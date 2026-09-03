//! The inverse of an applied plan: what `jails undo` hands the executor.
//!
//! **Every operation already carries both images**, because that is what
//! makes a plan reviewable: a replacement names the bytes it found and the
//! bytes it writes, a removal names what it took away, a published tree names
//! the digest it started from. So undoing an applied plan is not a new
//! computation over the model -- there is nothing to compile and nothing to
//! decide. It is the same bundle read backwards.
//!
//! **And it goes through the one executor**, which is what makes it safe.
//! The inverse plan's preconditions are the applied plan's *after* images, so
//! a file edited since is a stale precondition and the whole thing refuses
//! with nothing written -- the same refusal a plan reviewed and then
//! overtaken gets. Undo does not need to know what a merge is or which files
//! a reader touched; it needs the executor to check, and the executor checks.
//!
//! The one shape that is not a swap is a file the plan *created*: its
//! `before` is `None`, so the inverse is a removal rather than a replacement
//! back to bytes that never existed.

use crate::materialize::{BUNDLE_SCHEMA, PLAN_SCHEMA, plan_digest, tree_id};
use jails_contracts::{
    FileImageRef, FilePrecondition, Plan, PlanBundle, PlanInput, PlannedOperation,
    SnapshotPreconditions, TreeManifest,
};
use jails_model::Diagnostic;
use std::collections::BTreeMap;

/// The plan that puts back what this one wrote.
pub fn invert(bundle: &PlanBundle) -> Result<PlanBundle, Diagnostic> {
    let mut trees = bundle.trees.clone();
    let mut base = SnapshotPreconditions::default();
    let mut operations = Vec::new();
    // Reverse order, so a plan that wrote a file and then patched a document
    // that names it undoes the patch first. Nothing in the current operation
    // set depends on it, and a plan read backwards should be read backwards.
    for operation in bundle.plan.operations.iter().rev() {
        operations.push(inverse_of(operation, &mut trees, &mut base)?);
    }
    let summary = jails_contracts::SemanticPlan {
        model_nodes: 0,
        managed_files: operations.len(),
        migrations: 0,
        reader_document_intents: 0,
        effects: 0,
    };
    // **No follow-up effects.** They are what a capability asked to happen
    // *after* a plan -- a formatter run, a container start -- and undoing the
    // writes does not undo those. Silently re-running one would be worse than
    // leaving it: `add db` started a database, and `undo` removing the files
    // does not stop it.
    let input = PlanInput::undo();
    let digest = plan_digest(
        &bundle.plan.compiler,
        &base,
        &input,
        &summary,
        &operations,
        &[],
    )?;
    let plan = Plan {
        schema: PLAN_SCHEMA.to_string(),
        id: digest.as_str().to_string(),
        compiler: bundle.plan.compiler.clone(),
        base,
        input,
        summary,
        operations,
        follow_up_effects: Vec::new(),
        digest,
    };
    let inverted = PlanBundle {
        schema: BUNDLE_SCHEMA.to_string(),
        plan,
        trees,
        blobs: bundle.blobs.clone(),
    };
    crate::verify::verify_bundle(&inverted)?;
    Ok(inverted)
}

fn inverse_of(
    operation: &PlannedOperation,
    trees: &mut BTreeMap<jails_contracts::ContentDigest, TreeManifest>,
    base: &mut SnapshotPreconditions,
) -> Result<PlannedOperation, Diagnostic> {
    Ok(match operation {
        PlannedOperation::ReplaceModelFile {
            path,
            before,
            after,
        } => {
            base.files.insert(path.clone(), present(after));
            match before {
                Some(before) => PlannedOperation::ReplaceModelFile {
                    path: path.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                },
                None => PlannedOperation::RemoveFile {
                    path: path.clone(),
                    before: after.clone(),
                },
            }
        }
        PlannedOperation::ReplaceStateFile {
            path,
            before,
            after,
        } => {
            base.files.insert(path.clone(), present(after));
            match before {
                Some(before) => PlannedOperation::ReplaceStateFile {
                    path: path.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                },
                None => PlannedOperation::RemoveFile {
                    path: path.clone(),
                    before: after.clone(),
                },
            }
        }
        PlannedOperation::PatchReaderFile {
            path,
            before,
            after,
        } => {
            base.files.insert(path.clone(), present(after));
            match before {
                Some(before) => PlannedOperation::PatchReaderFile {
                    path: path.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                },
                None => PlannedOperation::RemoveFile {
                    path: path.clone(),
                    before: after.clone(),
                },
            }
        }
        PlannedOperation::RemoveFile { path, before } => {
            base.files.insert(path.clone(), FilePrecondition::Missing);
            PlannedOperation::PatchReaderFile {
                path: path.clone(),
                before: None,
                after: before.clone(),
            }
        }
        // **A migration is appended, and undoing it takes the file away.**
        // Not because history is reversible -- it is not, which is why
        // `AppendMigration` has no before-image -- but because this one was
        // written by the command being undone and has not been applied to a
        // database by jails. A migration already run against a database is a
        // different question and `undo` is not it: this removes a file the
        // last command created, in the same breath as the declaration that
        // asked for it.
        PlannedOperation::AppendMigration { path, after } => {
            base.files.insert(path.clone(), present(after));
            PlannedOperation::RemoveFile {
                path: path.clone(),
                before: after.clone(),
            }
        }
        PlannedOperation::PublishMergedTree {
            root,
            before,
            after,
        } => {
            let previous = match before {
                Some(before) => before.clone(),
                // The tree did not exist, so its inverse publishes nothing
                // over it -- which is how `publish_merged_tree` deletes every
                // path the applied tree wrote.
                None => {
                    let empty = TreeManifest {
                        entries: BTreeMap::new(),
                    };
                    let id = tree_id(&empty)?;
                    trees.entry(id.clone()).or_insert(empty);
                    id
                }
            };
            for (path, entry) in trees
                .get(after)
                .map(|tree| tree.entries.clone())
                .unwrap_or_default()
            {
                base.files.insert(
                    path,
                    FilePrecondition::Present {
                        digest: entry.blob,
                        executable: entry.mode == jails_contracts::FileMode::Executable,
                    },
                );
            }
            PlannedOperation::PublishMergedTree {
                root: root.clone(),
                before: Some(after.clone()),
                after: previous,
            }
        }
    })
}

fn present(image: &FileImageRef) -> FilePrecondition {
    FilePrecondition::Present {
        digest: image.blob.clone(),
        executable: image.mode == jails_contracts::FileMode::Executable,
    }
}
