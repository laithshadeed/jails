//! `.jails/compiler.lock.json`: what the next compile diffs against.
//!
//! Split out of `materialize.rs` for the board's largest-module rung, and it
//! is one subject: the accepted model and projection, each sealed by its own
//! digest, the published migrations, and the one encoder `relocate` shares so
//! a path rewrite writes the same file a plan does.

use super::*;

/// The lock's bytes: the accepted model and projection, each sealed by its
/// digest, and the published migrations. One encoder, so `relocate` -- which
/// rewrites the projection's paths and nothing else -- writes the same file
/// every plan does.
pub(crate) fn encode_compiler_lock(
    compiler: &str,
    model: &jails_model::AppModel,
    projection: &jails_contracts::RenderedTree,
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
) -> Result<Vec<u8>, Diagnostic> {
    let model_bytes = model.canonical_json().map_err(|error| {
        lock_encoding(format!("could not encode accepted compiler model: {error}"))
    })?;
    let model_digest = digest(&model_bytes)?;
    let projection_bytes = serde_json::to_vec(projection).map_err(|error| {
        lock_encoding(format!(
            "could not encode the accepted generated tree: {error}"
        ))
    })?;
    let projection_digest = digest(&projection_bytes)?;
    // **The digest is over the whole tree; the file records the parts.**
    // `projection_bytes` stays exactly what `serde` derives, so a lock
    // written by any release is checked under one rule -- and what this file
    // holds is one row per managed path with the digest of bytes that live
    // beside it under `.jails/base`. A reader rebuilds the tree from those
    // and recomputes this digest; the two halves cannot drift, because the
    // digest is of the whole thing.
    let mut lock = serde_json::to_value(CompilerLock {
        schema: COMPILER_LOCK_SCHEMA,
        compiler,
        model_digest,
        model,
        projection_digest,
        base: base_manifest(projection)?,
        migrations,
        migration_bytes,
    })
    .map_err(|error| lock_encoding(format!("could not encode compiler lock: {error}")))?;
    jails_contracts::lock_bytes::compact(&mut lock);
    serde_json::to_vec_pretty(&lock)
        .map_err(|error| lock_encoding(format!("could not encode compiler lock: {error}")))
}

/// The merge base as a tree of files: every managed path's accepted bytes,
/// written under [`BASE_ROOT`].
pub(crate) fn base_tree(
    projection: &jails_contracts::RenderedTree,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
) -> Result<jails_contracts::TreeManifest, Diagnostic> {
    let mut entries = BTreeMap::new();
    for (path, file) in &projection.files {
        let blob = digest(&file.bytes)?;
        blobs
            .entry(blob.clone())
            .or_insert_with(|| file.bytes.clone());
        entries.insert(
            base_path(path)?,
            jails_contracts::TreeEntry {
                kind: file.kind,
                mode: file.mode,
                blob,
            },
        );
    }
    Ok(jails_contracts::TreeManifest { entries })
}

/// `src/main/java/X.java` under the base root.
pub(crate) fn base_path(path: &ProjectPath) -> Result<ProjectPath, Diagnostic> {
    crate::capture::project_path(format!("{BASE_ROOT}/{path}"))
}

/// The accepted projection, as metadata plus digests.
#[derive(Serialize)]
pub(super) struct BaseManifest<'a> {
    files: BTreeMap<&'a ProjectPath, BaseEntry<'a>>,
    /// Kept inline, because a facet is a *span* of a reader-owned document
    /// rather than a file: there is nowhere in a tree of files to put one.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    reader_facets: &'a BTreeMap<String, jails_contracts::RenderedReaderFacet>,
}

#[derive(Serialize)]
struct BaseEntry<'a> {
    kind: &'a jails_contracts::FileKind,
    mode: &'a jails_contracts::FileMode,
    provenance: &'a jails_contracts::Provenance,
    digest: ContentDigest,
}

/// One row per managed path: what the file is, and the digest of the bytes
/// under `.jails/base` that are it.
fn base_manifest(
    projection: &jails_contracts::RenderedTree,
) -> Result<BaseManifest<'_>, Diagnostic> {
    let mut files = BTreeMap::new();
    for (path, file) in &projection.files {
        files.insert(
            path,
            BaseEntry {
                kind: &file.kind,
                mode: &file.mode,
                provenance: &file.provenance,
                digest: digest(&file.bytes)?,
            },
        );
    }
    Ok(BaseManifest {
        files,
        reader_facets: &projection.reader_facets,
    })
}

/// The lock would not serialise. One code for the three halves it is made of,
/// because a reader can do nothing different about any of them.
fn lock_encoding(message: String) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-lock-encoding",
        crate::capture::COMPILER_LOCK,
        message,
    )
}

/// Whether the lock on disk was written from exactly this state.
///
/// The schema is checked as well as the state, because a lock a previous
/// release wrote decodes to the same values and holds different bytes; a
/// project that never re-encoded would never migrate.
///
/// The marker is looked for in the whole file rather than at a fixed offset:
/// the lock is written through a `serde_json::Value`, whose maps are sorted,
/// so `schema` is neither first nor at any place worth relying on. A scan of
/// two megabytes is a couple of milliseconds against the hundred the encode
/// costs.
fn accepted_lock_is_current(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    compiler_version: &str,
    migrations: &BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: &BTreeMap<ProjectPath, Vec<u8>>,
) -> bool {
    let Ok(path) = crate::capture::project_path(crate::capture::COMPILER_LOCK) else {
        return false;
    };
    let Some(file) = snapshot.files.get(&path) else {
        return false;
    };
    if !file
        .bytes
        .windows(COMPILER_LOCK_SCHEMA.len())
        .any(|window| window == COMPILER_LOCK_SCHEMA.as_bytes())
    {
        return false;
    }
    snapshot.accepted_compiler.as_deref() == Some(compiler_version)
        && snapshot.accepted_model.as_ref() == Some(&draft.next_model)
        && snapshot.accepted_projection.as_ref() == Some(&draft.generated)
        && &snapshot.accepted_migrations == migrations
        && &snapshot.accepted_migration_bytes == migration_bytes
}

pub(super) fn materialize_compiler_lock(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    compiler_version: &str,
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), Diagnostic> {
    let path = crate::capture::project_path(crate::capture::COMPILER_LOCK)?;
    // **The encoder is a pure function of exactly these five values.** When
    // every one of them is what the file on disk was written from, the bytes
    // it would produce are the bytes already there -- so the comparison below
    // is decided without doing the work.
    //
    // It is worth deciding early because the encoding is the expensive half
    // of a plan: the projection is serialised once as fourteen megabytes of
    // JSON for the digest and once more into a `Value` tree for the file, and
    // on a hundred-entity project that was 116 ms of a 232 ms mutation, paid
    // in full by a run that changes nothing.
    if accepted_lock_is_current(
        snapshot,
        draft,
        compiler_version,
        &migrations,
        &migration_bytes,
    ) {
        return Ok(());
    }
    let lock_bytes = encode_compiler_lock(
        compiler_version,
        &draft.next_model,
        &draft.generated,
        migrations,
        migration_bytes,
    )?;
    let before = snapshot
        .files
        .get(&path)
        .map(|file| {
            file_image(
                &file.bytes,
                if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
                blobs,
            )
        })
        .transpose()?;
    let after = file_image(&lock_bytes, FileMode::Regular, blobs)?;
    if before.as_ref() == Some(&after) {
        return Ok(());
    }
    operations.push(PlannedOperation::ReplaceStateFile {
        path,
        before,
        after,
    });
    Ok(())
}
