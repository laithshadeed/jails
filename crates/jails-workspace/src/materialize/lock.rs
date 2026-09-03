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
    // **The digest above is the preimage; the bytes below are the file.**
    // `projection_bytes` is what the reader recomputes and compares, so it
    // stays exactly what `serde` derives and a lock written by any release
    // verifies under one rule. What the file holds is the compact form, where
    // a generated file's bytes are the text they are rather than four
    // characters per byte (`jails_contracts::lock_bytes`).
    let mut lock = serde_json::to_value(CompilerLock {
        schema: COMPILER_LOCK_SCHEMA,
        compiler,
        model_digest,
        model,
        projection_digest,
        projection,
        migrations,
        migration_bytes,
    })
    .map_err(|error| lock_encoding(format!("could not encode compiler lock: {error}")))?;
    jails_contracts::lock_bytes::compact(&mut lock);
    serde_json::to_vec_pretty(&lock)
        .map_err(|error| lock_encoding(format!("could not encode compiler lock: {error}")))
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

pub(super) fn materialize_compiler_lock(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    compiler_version: &str,
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), Diagnostic> {
    let lock_bytes = encode_compiler_lock(
        compiler_version,
        &draft.next_model,
        &draft.generated,
        migrations,
        migration_bytes,
    )?;
    let path = crate::capture::project_path(crate::capture::COMPILER_LOCK)?;
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
