//! Three-way reconciliation of compiler artifacts with the live managed files.

use jails_contracts::{
    ContentDigest, DocumentIntent, FileKind, FileMode, PlanDraft, ProjectPath, RenderedFile,
    RenderedTree, TreeEntry, TreeManifest, WorkspaceSnapshot,
};
use jails_model::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};

type ArtifactFile<'a> = (&'a ProjectPath, &'a RenderedFile);

struct Adoption {
    source: ProjectPath,
    base: Vec<u8>,
}

/// The managed tree before and after this plan.
///
/// **`before` is the managed set as captured**: every path the accepted
/// projection names and the capture found, with the bytes it found -- OURS.
/// **`after` is the reconciled result.** The executor publishes `after` and
/// deletes what is in `before` and not in `after`; nothing else on disk is
/// its business, because managed files sit beside the reader's own under
/// `src/` and only the projection says which is which.
///
/// An ejected boundary's files are in neither: they leave the projection and
/// stay where they are, which is already reader source.
pub(crate) fn trees(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    restore: crate::materialize::Restore,
) -> Result<(TreeManifest, TreeManifest), Diagnostic> {
    let mut before = TreeManifest::default();
    let mut tree = TreeManifest::default();
    let baseline = artifact_index(&draft.baseline)?;
    let desired = artifact_index(&draft.generated)?;
    let artifact_ids = baseline
        .keys()
        .chain(desired.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let ejected_boundaries = draft
        .next_model
        .ejections
        .values()
        .map(|ejection| ejection.target.as_str())
        .collect::<BTreeSet<_>>();
    let adopted_sources = adoption_index(&draft.reader_document_intents)?;
    let mut adopted_targets = BTreeSet::new();

    for artifact_id in artifact_ids {
        let base = baseline.get(&artifact_id).copied();
        let theirs = desired.get(&artifact_id).copied();
        let ejected = base.is_some_and(|(_, file)| {
            ejected_boundaries.contains(file.provenance.ejection_target())
        });
        if let Some((path, _)) = base
            && !ejected
            && let Some(live) = snapshot.files.get(path)
        {
            insert_tree_entry(
                &mut before,
                blobs,
                path.clone(),
                live.bytes.clone(),
                crate::materialize::file_kind(path),
                captured_mode(live),
            )?;
        }
        if let (Some((old_path, _)), Some((new_path, _))) = (base, theirs)
            && old_path != new_path
            && snapshot.files.contains_key(new_path)
        {
            return Err(Diagnostic::new(
                "workspace-rename-destination-exists",
                new_path.to_string(),
                format!(
                    "renamed artifact `{artifact_id}` cannot move to `{new_path}` because that path already exists"
                ),
                "move the destination file; nothing was written",
            ));
        }
        reconcile_artifact(
            snapshot,
            base,
            theirs,
            Ownership {
                ejected,
                adoption: theirs.and_then(|(path, _)| adopted_sources.get(path)),
                restore,
            },
            &mut tree,
            blobs,
        )?;
        if let Some((path, _)) = theirs
            && adopted_sources.contains_key(path)
        {
            adopted_targets.insert(path.clone());
        }
    }

    if adopted_targets.len() != adopted_sources.len() {
        let missing = adopted_sources
            .keys()
            .find(|path| !adopted_targets.contains(*path))
            .expect("different adoption sets have one missing target");
        return Err(Diagnostic::new(
            "workspace-adoption-target-unmodelled",
            missing.to_string(),
            format!("adoption target `{missing}` is not a compiler artifact"),
            "import only source units represented by the canonical model",
        ));
    }

    Ok((before, tree))
}

fn adoption_index(
    intents: &[DocumentIntent],
) -> Result<BTreeMap<ProjectPath, Adoption>, Diagnostic> {
    let mut targets = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for intent in intents {
        let DocumentIntent::AdoptJava { source, path, base } = intent else {
            continue;
        };
        if !sources.insert(source.clone()) {
            return Err(Diagnostic::without_a_fix(
                "workspace-adoption-source-repeated",
                source.to_string(),
                format!("reader source `{source}` is adopted more than once"),
            ));
        }
        if targets
            .insert(
                path.clone(),
                Adoption {
                    source: source.clone(),
                    base: base.clone(),
                },
            )
            .is_some()
        {
            return Err(Diagnostic::without_a_fix(
                "workspace-adoption-target-repeated",
                path.to_string(),
                format!("managed target `{path}` is adopted more than once"),
            ));
        }
    }
    Ok(targets)
}

fn artifact_index(tree: &RenderedTree) -> Result<BTreeMap<String, ArtifactFile<'_>>, Diagnostic> {
    let mut artifacts = BTreeMap::new();
    for (path, file) in &tree.files {
        let id = file.provenance.artifact_id.clone();
        if let Some((previous, _)) = artifacts.insert(id.clone(), (path, file)) {
            return Err(Diagnostic::new(
                "workspace-artifact-path-collision",
                path.to_string(),
                format!("compiler emitted artifact `{id}` at both `{previous}` and `{path}`"),
                "give every emitted file its own stable artifact id",
            ));
        }
    }
    Ok(artifacts)
}

/// How one artifact is reconciled, as against what it renders to.
///
/// The three travel together because each is a claim about *this path's*
/// ownership rather than about its content: whether the reader has taken the
/// implementation (`ejected`), whether a reader file is being imported into it
/// (`adoption`), and what a deletion of it means (`restore`).
struct Ownership<'a> {
    ejected: bool,
    adoption: Option<&'a Adoption>,
    restore: crate::materialize::Restore,
}

fn reconcile_artifact(
    snapshot: &WorkspaceSnapshot,
    base: Option<ArtifactFile<'_>>,
    desired: Option<ArtifactFile<'_>>,
    ownership: Ownership<'_>,
    tree: &mut TreeManifest,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
) -> Result<(), Diagnostic> {
    let Ownership {
        ejected,
        adoption,
        restore,
    } = ownership;
    let live_path = live_path(base, desired);
    let output_path = desired.map(|(path, _)| path).unwrap_or(live_path);
    let live = snapshot.files.get(live_path);
    let base_file = base.map(|(_, file)| file);
    let desired_file = desired.map(|(_, file)| file);
    let selected = if let Some(adoption) = adoption {
        if base_file.is_some() || live.is_some() {
            return Err(Diagnostic::new(
                "workspace-adoption-target-occupied",
                output_path.to_string(),
                format!(
                    "adoption target `{output_path}` already has canonical history or live bytes"
                ),
                "import only into a project with no canonical managed artifact",
            ));
        }
        let theirs = desired_file.expect("an adoption target was matched to desired artifact");
        let ours = snapshot.files.get(&adoption.source).ok_or_else(|| {
            Diagnostic::new(
                "workspace-adoption-source-uncaptured",
                adoption.source.to_string(),
                format!(
                    "reader source `{}` was not captured for adoption",
                    adoption.source
                ),
                "restore the file and re-plan the import",
            )
        })?;
        let bytes = if ours.bytes == adoption.base || ours.bytes == theirs.bytes {
            theirs.bytes.clone()
        } else {
            match crate::merge::three_way(output_path, &adoption.base, &ours.bytes, &theirs.bytes)?
            {
                crate::merge::Merged::Clean(bytes) => bytes,
                crate::merge::Merged::Conflicted { hunks } => {
                    return Err(Diagnostic::new(
                        "workspace-import-conflict",
                        adoption.source.to_string(),
                        format!(
                            "`{}` has {hunks} overlapping legacy-template and reader edit{} during import",
                            adoption.source,
                            if hunks == 1 { "" } else { "s" }
                        ),
                        "reconcile that component by hand; nothing was written",
                    ));
                }
            }
        };
        Some((bytes, theirs.kind, captured_mode(ours)))
    } else {
        match (base_file, live, desired_file) {
            (None, None, Some(theirs)) => Some((theirs.bytes.clone(), theirs.kind, theirs.mode)),
            (None, Some(_), Some(_)) => {
                return Err(Diagnostic::new(
                    "workspace-generated-path-reader-owned",
                    output_path.to_string(),
                    format!("generated path `{output_path}` is already reader-owned"),
                    "move the existing file or explicitly import it before generating",
                ));
            }
            (Some(base), Some(ours), Some(theirs)) if ours.bytes == base.bytes => {
                Some((theirs.bytes.clone(), theirs.kind, theirs.mode))
            }
            (Some(base), Some(ours), Some(theirs)) if theirs.bytes == base.bytes => {
                Some((ours.bytes.clone(), theirs.kind, captured_mode(ours)))
            }
            (Some(_), Some(ours), Some(theirs)) if ours.bytes == theirs.bytes => {
                Some((theirs.bytes.clone(), theirs.kind, captured_mode(ours)))
            }
            (Some(base), Some(ours), Some(theirs)) => {
                match crate::merge::three_way(output_path, &base.bytes, &ours.bytes, &theirs.bytes)?
                {
                    crate::merge::Merged::Clean(bytes) => {
                        Some((bytes, theirs.kind, captured_mode(ours)))
                    }
                    crate::merge::Merged::Conflicted { hunks } => {
                        return Err(Diagnostic::new(
                            "workspace-managed-file-conflict",
                            output_path.to_string(),
                            format!(
                                "`{output_path}` has {hunks} overlapping edit{} between your file and the generator",
                                if hunks == 1 { "" } else { "s" }
                            ),
                            "reconcile that component by hand; nothing was written",
                        ));
                    }
                }
            }
            (Some(base), Some(ours), None) if ours.bytes == base.bytes => None,
            (Some(_), Some(_), None) if ejected => None,
            (Some(_), Some(_), None)
                if restore == crate::materialize::Restore::EditedAndRemoved =>
            {
                None
            }
            (Some(_), Some(_), None) => {
                return Err(Diagnostic::new(
                    "workspace-managed-file-edited-and-removed",
                    live_path.to_string(),
                    format!("`{live_path}` was edited by you but removed by the generator"),
                    "move the custom code to reader source, keep the model component, or repeat with `--yes` to discard the edits; nothing was written",
                ));
            }
            // `resource repair` is the one plan that writes it back. A
            // managed file is reproducible by definition -- the model renders
            // it -- so there is nothing of the reader's left to lose once the
            // bytes are gone, and refusing forever would leave a project with
            // no way out of a deletion.
            (Some(_), None, Some(theirs)) if restore == crate::materialize::Restore::Deleted => {
                Some((theirs.bytes.clone(), theirs.kind, theirs.mode))
            }
            (Some(_), None, Some(_)) => {
                return Err(Diagnostic::new(
                    "workspace-managed-file-deleted",
                    live_path.to_string(),
                    format!(
                        "managed file `{live_path}` was deleted by you while the generator still needs it"
                    ),
                    "`jails resource repair` writes it back from the model, or eject its implementation boundary; nothing was written",
                ));
            }
            (Some(_), None, None) => None,
            // An artifact comes from BASE or THEIRS, so a live file with
            // neither is not an artifact at all: it is the reader's, and
            // outside this tree.
            (None, Some(_), None) | (None, None, None) => None,
        }
    };
    if let Some((bytes, kind, mode)) = selected {
        insert_tree_entry(tree, blobs, output_path.clone(), bytes, kind, mode)?;
    }
    Ok(())
}

fn live_path<'a>(
    base: Option<ArtifactFile<'a>>,
    desired: Option<ArtifactFile<'a>>,
) -> &'a ProjectPath {
    base.map(|(path, _)| path)
        .or_else(|| desired.map(|(path, _)| path))
        .expect("an artifact comes from baseline or desired")
}

fn insert_tree_entry(
    tree: &mut TreeManifest,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    path: ProjectPath,
    bytes: Vec<u8>,
    kind: FileKind,
    mode: FileMode,
) -> Result<(), Diagnostic> {
    let blob = crate::materialize::digest(&bytes)?;
    blobs.insert(blob.clone(), bytes);
    if tree
        .entries
        .insert(path.clone(), TreeEntry { kind, mode, blob })
        .is_some()
    {
        return Err(Diagnostic::new(
            "workspace-tree-path-collision",
            path.to_string(),
            format!("two reconciled artifacts target `{path}`"),
            "resolve the compiler path collision",
        ));
    }
    Ok(())
}

fn captured_mode(file: &jails_contracts::CapturedFile) -> FileMode {
    if file.executable {
        FileMode::Executable
    } else {
        FileMode::Regular
    }
}
