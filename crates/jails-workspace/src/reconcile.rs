//! Three-way reconciliation of compiler artifacts with the live managed tree.

use jails_contracts::{
    ContentDigest, DocumentIntent, FileKind, FileMode, PlanDraft, ProjectPath, RenderedFile,
    RenderedTree, TreeEntry, TreeManifest, WorkspaceSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};

type ArtifactFile<'a> = (&'a ProjectPath, &'a RenderedFile);

struct Adoption {
    source: ProjectPath,
    base: Vec<u8>,
}

pub(crate) fn tree(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    restore: crate::materialize::Restore,
) -> Result<TreeManifest, String> {
    let mut tree = TreeManifest::default();
    let baseline = artifact_index(&draft.baseline)?;
    let desired = artifact_index(&draft.generated)?;
    let artifact_ids = baseline
        .keys()
        .chain(desired.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut handled_paths = BTreeSet::new();
    let ejected_sources = draft
        .reader_document_intents
        .iter()
        .filter_map(|intent| match intent {
            DocumentIntent::EjectFile { source, .. } => Some(source),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let adopted_sources = adoption_index(&draft.reader_document_intents)?;
    let mut adopted_targets = BTreeSet::new();

    for artifact_id in artifact_ids {
        let base = baseline.get(&artifact_id).copied();
        let theirs = desired.get(&artifact_id).copied();
        if let Some((path, _)) = base {
            handled_paths.insert(path.clone());
        }
        if let Some((path, _)) = theirs {
            handled_paths.insert(path.clone());
        }
        if let (Some((old_path, _)), Some((new_path, _))) = (base, theirs)
            && old_path != new_path
            && snapshot.files.contains_key(new_path)
        {
            return Err(format!(
                "renamed artifact `{artifact_id}` cannot move to `{new_path}` because that path already exists\n       fix: move the destination file; nothing was written"
            ));
        }
        reconcile_artifact(
            snapshot,
            base,
            theirs,
            Ownership {
                ejected: ejected_sources.contains(live_path(base, theirs)),
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
        return Err(format!(
            "adoption target `{missing}` is not a compiler artifact\n       fix: import only source units represented by the canonical model"
        ));
    }

    for (path, live) in snapshot
        .files
        .iter()
        .filter(|(path, _)| path.is_within(&draft.generated.root))
    {
        if handled_paths.contains(path) {
            continue;
        }
        insert_tree_entry(
            &mut tree,
            blobs,
            path.clone(),
            live.bytes.clone(),
            crate::materialize::file_kind(path),
            captured_mode(live),
        )?;
    }
    Ok(tree)
}

fn adoption_index(intents: &[DocumentIntent]) -> Result<BTreeMap<ProjectPath, Adoption>, String> {
    let mut targets = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for intent in intents {
        let DocumentIntent::AdoptJava { source, path, base } = intent else {
            continue;
        };
        if !sources.insert(source.clone()) {
            return Err(format!(
                "reader source `{source}` is adopted more than once"
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
            return Err(format!("managed target `{path}` is adopted more than once"));
        }
    }
    Ok(targets)
}

fn artifact_index(tree: &RenderedTree) -> Result<BTreeMap<String, ArtifactFile<'_>>, String> {
    let mut artifacts = BTreeMap::new();
    for (path, file) in &tree.files {
        let id = file.provenance.artifact_id.clone();
        if let Some((previous, _)) = artifacts.insert(id.clone(), (path, file)) {
            return Err(format!(
                "compiler emitted artifact `{id}` at both `{previous}` and `{path}`\n       fix: give every emitted file its own stable artifact id"
            ));
        }
    }
    Ok(artifacts)
}

/// How one artifact is reconciled, as against what it renders to.
///
/// The three travel together because each is a claim about *this path's*
/// ownership rather than about its content: whether the reader has taken the
/// implementation (`ejected`), whether a legacy file is being imported into it
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
) -> Result<(), String> {
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
            return Err(format!(
                "adoption target `{output_path}` already has canonical history or live bytes\n       fix: import only into a project with no canonical managed artifact"
            ));
        }
        let theirs = desired_file.expect("an adoption target was matched to desired artifact");
        let ours = snapshot.files.get(&adoption.source).ok_or_else(|| {
            format!(
                "reader source `{}` was not captured for adoption\n       fix: restore the file and re-plan the import",
                adoption.source
            )
        })?;
        let bytes = if ours.bytes == adoption.base || ours.bytes == theirs.bytes {
            theirs.bytes.clone()
        } else {
            match crate::merge::three_way(output_path, &adoption.base, &ours.bytes, &theirs.bytes)?
            {
                crate::merge::Merged::Clean(bytes) => bytes,
                crate::merge::Merged::Conflicted { hunks } => {
                    return Err(format!(
                        "`{}` has {hunks} overlapping legacy-template and reader edit{} during import\n       fix: reconcile that component by hand; nothing was written",
                        adoption.source,
                        if hunks == 1 { "" } else { "s" }
                    ));
                }
            }
        };
        Some((bytes, theirs.kind, captured_mode(ours)))
    } else {
        match (base_file, live, desired_file) {
            (None, None, Some(theirs)) => Some((theirs.bytes.clone(), theirs.kind, theirs.mode)),
            (None, Some(_), Some(_)) => {
                return Err(format!(
                    "generated path `{output_path}` is already reader-owned\n       fix: move the existing file or explicitly import it before generating"
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
                        return Err(format!(
                            "`{output_path}` has {hunks} overlapping edit{} between your file and the generator\n       fix: reconcile that component by hand; nothing was written",
                            if hunks == 1 { "" } else { "s" }
                        ));
                    }
                }
            }
            (Some(base), Some(ours), None) if ours.bytes == base.bytes => None,
            (Some(_), Some(_), None) if ejected => None,
            (Some(_), Some(_), None) => {
                return Err(format!(
                    "`{live_path}` was edited by you but removed by the generator\n       fix: move the custom code to reader source or keep the model component; nothing was written"
                ));
            }
            // `resource repair` is the one plan that writes it back. A
            // managed file is reproducible by definition -- the model renders
            // it -- so there is nothing of the reader's left to lose once the
            // bytes are gone, and refusing forever is what left a canonical
            // project with no way out of a deletion.
            (Some(_), None, Some(theirs)) if restore == crate::materialize::Restore::Deleted => {
                Some((theirs.bytes.clone(), theirs.kind, theirs.mode))
            }
            (Some(_), None, Some(_)) => {
                return Err(format!(
                    "managed file `{live_path}` was deleted by you while the generator still needs it\n       fix: `jails resource repair` writes it back from the model, or eject its implementation boundary; nothing was written"
                ));
            }
            (Some(_), None, None) => None,
            (None, Some(ours), None) => Some((
                ours.bytes.clone(),
                crate::materialize::file_kind(live_path),
                captured_mode(ours),
            )),
            (None, None, None) => None,
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
) -> Result<(), String> {
    let blob = crate::materialize::digest(&bytes)?;
    blobs.insert(blob.clone(), bytes);
    if tree
        .entries
        .insert(path.clone(), TreeEntry { kind, mode, blob })
        .is_some()
    {
        return Err(format!(
            "two reconciled artifacts target `{path}`\n       fix: resolve the compiler path collision"
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
