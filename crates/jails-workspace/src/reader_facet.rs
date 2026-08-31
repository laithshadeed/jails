//! Merge-managed compiler slices in reader-owned project files.
//!
//! Facets retain only their accepted generator bytes as BASE. Compose facets
//! own one marked slice of a larger document; managed-file facets own one
//! complete project file. Both use the same BASE/OURS/THEIRS refusal contract.

use jails_contracts::{
    ContentDigest, FileMode, PlannedOperation, ProjectPath, ReaderFacetKind, RenderedReaderFacet,
    WorkspaceSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};

enum ManagedFileMerge {
    Unchanged,
    Write(Vec<u8>),
    Remove,
}

pub(crate) fn materialize(
    snapshot: &WorkspaceSnapshot,
    baseline: &BTreeMap<String, RenderedReaderFacet>,
    generated: &BTreeMap<String, RenderedReaderFacet>,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
    restore: crate::materialize::Restore,
) -> Result<(), String> {
    let ids = baseline
        .keys()
        .chain(generated.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut documents = BTreeMap::<ProjectPath, Vec<u8>>::new();
    let mut managed_files = BTreeSet::<ProjectPath>::new();
    for id in ids {
        let base = baseline.get(&id);
        let desired = generated.get(&id);
        let path = base
            .map(|facet| &facet.path)
            .or_else(|| desired.map(|facet| &facet.path))
            .expect("reader facet id came from baseline or generated");
        if let (Some(base), Some(desired)) = (base, desired)
            && (base.path != desired.path || base.kind != desired.kind)
        {
            return Err(format!(
                "reader facet `{id}` changed document identity\n       fix: keep its path and kind stable or introduce a new facet id"
            ));
        }
        let kind = desired
            .map(|facet| &facet.kind)
            .or_else(|| base.map(|facet| &facet.kind))
            .expect("reader facet has a baseline or desired kind");
        match kind {
            ReaderFacetKind::ComposeService { service, marker } => {
                let current = documents
                    .get(path)
                    .map(Vec::as_slice)
                    .or_else(|| snapshot.files.get(path).map(|file| file.bytes.as_slice()))
                    .unwrap_or_default();
                let text = std::str::from_utf8(current).map_err(|_| {
                    format!(
                        "reader document `{path}` is not UTF-8\n       fix: restore a UTF-8 document before compiling model facets"
                    )
                })?;
                if base.is_none()
                    && desired.is_some()
                    && compose_service_without_marker(text, service, marker)
                {
                    return Err(format!(
                        "compose service `{service}` already exists outside `{}{marker}` in `{path}`\n       fix: rename or remove the reader-owned service before generating",
                        jails_codemod::Marked::OPEN_PREFIX
                    ));
                }
                let updated = crate::documents::reconcile_compose_service(
                    path,
                    text,
                    service,
                    marker,
                    base.map(|facet| facet.bytes.as_slice()),
                    desired.map(|facet| facet.bytes.as_slice()),
                )?;
                documents.insert(path.clone(), updated.into_bytes());
            }
            ReaderFacetKind::ManagedFile { mode } => {
                if !managed_files.insert(path.clone()) {
                    return Err(format!(
                        "two managed project-file facets target `{path}`\n       fix: give every external artifact one stable path"
                    ));
                }
                let current = snapshot.files.get(path);
                match reconcile_managed_file(
                    path,
                    base.map(|facet| facet.bytes.as_slice()),
                    current.map(|file| file.bytes.as_slice()),
                    desired.map(|facet| facet.bytes.as_slice()),
                    restore,
                )? {
                    ManagedFileMerge::Unchanged => {}
                    ManagedFileMerge::Write(bytes) => {
                        let before = current
                            .map(|file| {
                                crate::materialize::file_image(
                                    &file.bytes,
                                    captured_mode(file),
                                    blobs,
                                )
                            })
                            .transpose()?;
                        let after = crate::materialize::file_image(&bytes, *mode, blobs)?;
                        operations.push(PlannedOperation::PatchReaderFile {
                            path: path.clone(),
                            before,
                            after,
                        });
                    }
                    ManagedFileMerge::Remove => {
                        let current = current.expect("removal has one captured current file");
                        let before = crate::materialize::file_image(
                            &current.bytes,
                            captured_mode(current),
                            blobs,
                        )?;
                        operations.push(PlannedOperation::RemoveReaderFile {
                            path: path.clone(),
                            before,
                        });
                    }
                }
            }
        }
    }

    for (path, after_bytes) in documents {
        let before_file = snapshot.files.get(&path);
        if before_file.is_some_and(|file| file.bytes == after_bytes)
            || before_file.is_none() && after_bytes.is_empty()
        {
            continue;
        }
        let mode = before_file.map_or(FileMode::Regular, captured_mode);
        let before = before_file
            .map(|file| crate::materialize::file_image(&file.bytes, mode, blobs))
            .transpose()?;
        let after = crate::materialize::file_image(&after_bytes, mode, blobs)?;
        operations.push(PlannedOperation::PatchReaderFile {
            path,
            before,
            after,
        });
    }
    Ok(())
}

fn reconcile_managed_file(
    path: &ProjectPath,
    base: Option<&[u8]>,
    current: Option<&[u8]>,
    desired: Option<&[u8]>,
    restore: crate::materialize::Restore,
) -> Result<ManagedFileMerge, String> {
    match (base, current, desired) {
        (None, None, Some(desired)) => Ok(ManagedFileMerge::Write(desired.to_vec())),
        (None, Some(_), Some(_)) => Err(format!(
            "managed project path `{path}` is already reader-owned\n       fix: move the existing file before generating; nothing was written"
        )),
        (Some(base), Some(current), Some(desired)) => {
            let merged = if current == base || current == desired {
                desired.to_vec()
            } else {
                match crate::merge::three_way(path, base, current, desired)? {
                    crate::merge::Merged::Clean(bytes) => bytes,
                    crate::merge::Merged::Conflicted { hunks } => {
                        return Err(format!(
                            "`{path}` has {hunks} overlapping edit{} between your file and the generator\n       fix: reconcile that project file by hand; nothing was written",
                            if hunks == 1 { "" } else { "s" }
                        ));
                    }
                }
            };
            if merged == current {
                Ok(ManagedFileMerge::Unchanged)
            } else {
                Ok(ManagedFileMerge::Write(merged))
            }
        }
        // See `reconcile.rs`: `resource repair` writes it back.
        (Some(_), None, Some(desired)) if restore == crate::materialize::Restore::Deleted => {
            Ok(ManagedFileMerge::Write(desired.to_vec()))
        }
        (Some(_), None, Some(_)) => Err(format!(
            "managed project file `{path}` was deleted by you while the generator still needs it\n       fix: `jails resource repair` writes it back from the model, or remove the owning model component; nothing was written"
        )),
        (Some(base), Some(current), None) if current == base => Ok(ManagedFileMerge::Remove),
        (Some(_), Some(_), None) => Err(format!(
            "`{path}` was edited by you but removed by the generator\n       fix: move the custom content to another file or keep the model component; nothing was written"
        )),
        (Some(_), None, None) | (None, None, None) | (None, Some(_), None) => {
            Ok(ManagedFileMerge::Unchanged)
        }
    }
}

fn captured_mode(file: &jails_contracts::CapturedFile) -> FileMode {
    if file.executable {
        FileMode::Executable
    } else {
        FileMode::Regular
    }
}

fn compose_service_without_marker(text: &str, service: &str, marker: &str) -> bool {
    let marker = jails_codemod::Marked::new(marker).open();
    if text.lines().any(|line| line.trim() == marker) {
        return false;
    }
    let expected = format!("  {service}:");
    let mut in_services = false;
    for line in text.lines() {
        if line.trim_end() == "services:" && !line.starts_with(' ') {
            in_services = true;
            continue;
        }
        if in_services && !line.is_empty() && !line.starts_with(' ') && !line.starts_with('#') {
            break;
        }
        if in_services && line.trim_end() == expected {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> ProjectPath {
        ProjectPath::parse("load-tests/api.js").unwrap()
    }

    #[test]
    fn managed_file_three_way_merge_keeps_disjoint_reader_and_generator_edits() {
        let base = b"const routes = [];\n// reader area\n";
        let current = b"const routes = [];\n// reader area\nexport const token = 'mine';\n";
        let desired = b"const routes = ['GET /tasks'];\n// reader area\n";
        let ManagedFileMerge::Write(merged) = reconcile_managed_file(
            &path(),
            Some(base),
            Some(current),
            Some(desired),
            crate::materialize::Restore::Refuse,
        )
        .unwrap() else {
            panic!("expected a merged write")
        };
        let merged = String::from_utf8(merged).unwrap();
        assert!(merged.contains("GET /tasks"), "{merged}");
        assert!(merged.contains("token = 'mine'"), "{merged}");
    }

    #[test]
    fn managed_file_refuses_overlap_collision_deletion_and_edited_removal() {
        let error = reconcile_managed_file(
            &path(),
            Some(b"const route = 'old';\n"),
            Some(b"const route = 'reader';\n"),
            Some(b"const route = 'generator';\n"),
            crate::materialize::Restore::Refuse,
        )
        .err()
        .unwrap();
        assert!(error.contains("overlapping"), "{error}");
        assert!(error.contains("nothing was written"), "{error}");

        let collision = reconcile_managed_file(
            &path(),
            None,
            Some(b"mine\n"),
            Some(b"theirs\n"),
            crate::materialize::Restore::Refuse,
        )
        .err()
        .unwrap();
        assert!(collision.contains("reader-owned"), "{collision}");

        let deletion = reconcile_managed_file(
            &path(),
            Some(b"base\n"),
            None,
            Some(b"next\n"),
            crate::materialize::Restore::Refuse,
        )
        .err()
        .unwrap();
        assert!(deletion.contains("deleted by you"), "{deletion}");

        let removal = reconcile_managed_file(
            &path(),
            Some(b"base\n"),
            Some(b"reader edit\n"),
            None,
            crate::materialize::Restore::Refuse,
        )
        .err()
        .unwrap();
        assert!(removal.contains("edited by you"), "{removal}");
    }
}
