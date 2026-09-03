//! Checking that an exact plan is internally consistent before anything acts
//! on it.
//!
//! **Split out by direction.** Everything in `materialize` builds a bundle
//! from a snapshot and a draft; this reads one back and asks whether it holds
//! together -- every blob matching its digest, every tree entry pointing at a
//! blob the bundle carries, every before- and after-image resolvable. That is
//! the check an *exported* bundle needs, where the producer is not this
//! process and may not be this version.
//!
//! It is not verification of the *plan against the project*: whether the
//! preconditions still hold is the executor\'s question, asked under the lock
//! at the moment of writing. This one is about the document alone.

use super::materialize::{BUNDLE_SCHEMA, PLAN_SCHEMA, digest, plan_digest, tree_id};
use jails_contracts::{FileImageRef, PlanBundle, PlannedOperation};
use jails_model::Diagnostic;

pub fn verify_bundle(bundle: &PlanBundle) -> Result<(), Diagnostic> {
    if bundle.schema != BUNDLE_SCHEMA || bundle.plan.schema != PLAN_SCHEMA {
        return Err(Diagnostic::without_a_fix(
            "workspace-bundle-schema",
            "$.plan",
            "unsupported plan schema",
        ));
    }
    for (id, bytes) in &bundle.blobs {
        if &digest(bytes)? != id {
            return Err(Diagnostic::without_a_fix(
                "workspace-blob-content-mismatch",
                format!("$.blobs.{}", id.as_str()),
                format!("blob `{id:?}` does not match its content"),
            ));
        }
    }
    for (id, tree) in &bundle.trees {
        for entry in tree.entries.values() {
            if !bundle.blobs.contains_key(&entry.blob) {
                return Err(Diagnostic::without_a_fix(
                    "workspace-tree-blob-missing",
                    format!("$.trees.{}", id.as_str()),
                    format!("tree `{id:?}` references a missing blob"),
                ));
            }
        }
        if &tree_id(tree)? != id {
            return Err(Diagnostic::without_a_fix(
                "workspace-tree-manifest-mismatch",
                format!("$.trees.{}", id.as_str()),
                format!("tree `{id:?}` does not match its manifest"),
            ));
        }
    }
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree { before, after, .. } => {
                if before
                    .as_ref()
                    .is_some_and(|tree| !bundle.trees.contains_key(tree))
                    || !bundle.trees.contains_key(after)
                {
                    return Err(Diagnostic::without_a_fix(
                        "workspace-tree-missing",
                        "$.plan.operations",
                        "managed-tree operation references a missing tree",
                    ));
                }
            }
            PlannedOperation::ReplaceModelFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::PatchReaderFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::RemoveReaderFile { before, .. } => verify_image(bundle, before)?,
            PlannedOperation::ReplaceStateFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::AppendMigration { after, .. } => verify_image(bundle, after)?,
        }
    }
    let actual = plan_digest(
        &bundle.plan.compiler,
        &bundle.plan.base,
        &bundle.plan.input,
        &bundle.plan.summary,
        &bundle.plan.operations,
        &bundle.plan.follow_up_effects,
    )?;
    if actual != bundle.plan.digest || bundle.plan.id != actual.as_str() {
        return Err(Diagnostic::without_a_fix(
            "workspace-plan-digest-mismatch",
            "$.plan",
            "plan digest does not match the plan",
        ));
    }
    Ok(())
}

fn verify_image(bundle: &PlanBundle, image: &FileImageRef) -> Result<(), Diagnostic> {
    let bytes = bundle.blobs.get(&image.blob).ok_or_else(|| {
        Diagnostic::without_a_fix(
            "workspace-image-blob-missing",
            format!("$.blobs.{}", image.blob.as_str()),
            format!(
                "file image references missing blob `{}`",
                image.blob.as_str()
            ),
        )
    })?;
    if bytes.len() as u64 != image.len {
        return Err(Diagnostic::without_a_fix(
            "workspace-image-length-mismatch",
            format!("$.blobs.{}", image.blob.as_str()),
            format!("file image `{}` has the wrong length", image.blob.as_str()),
        ));
    }
    Ok(())
}
