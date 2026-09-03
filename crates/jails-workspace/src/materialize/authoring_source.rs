//! Publishing the model file, and retiring the one it replaces.
//!
//! **One concern, and it is not the managed tree's.** Everything else
//! [`super::materialize_with_model`] does is about reproducible output --
//! rendering it, merging it, addressing it. This is about the *authoring
//! source*: the file the reader edits, which the compiler reads and never
//! writes except here.
//!
//! **The retirement has to be in the same plan as the replacement.** Its one
//! caller is the upgrade that moves a project off `.jails/model.toml`, and
//! writing the JDL without retiring the TOML leaves two editable model
//! sources -- the state `docs/00-contracts.md` forbids. Two plans would leave
//! a window, and a crash inside it would make that state permanent.

use super::{digest, file_image};
use jails_contracts::{
    ContentDigest, FileImageRef, FileMode, ModelFileUpdate, PlannedOperation, WorkspaceSnapshot,
};
use jails_model::Diagnostic;
use std::collections::BTreeMap;

pub(super) fn publish_authoring_source(
    snapshot: &WorkspaceSnapshot,
    model_update: Option<ModelFileUpdate>,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), Diagnostic> {
    if let Some(update) = model_update {
        let before = snapshot.files.get(&update.path).map(|file| {
            let blob = digest(&file.bytes)?;
            blobs.insert(blob.clone(), file.bytes.clone());
            Ok::<_, Diagnostic>(FileImageRef {
                blob,
                len: file.bytes.len() as u64,
                mode: if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
            })
        });
        let before = before.transpose()?;
        let after_blob = digest(&update.bytes)?;
        blobs.insert(after_blob.clone(), update.bytes.clone());
        operations.push(PlannedOperation::ReplaceModelFile {
            path: update.path,
            before,
            after: FileImageRef {
                blob: after_blob,
                len: update.bytes.len() as u64,
                mode: FileMode::Regular,
            },
        });
        // **In the same plan, or the project ends with two model sources.**
        // The upgrade that moves a project off `.jails/model.toml` writes the
        // JDL and retires the TOML; splitting those across two plans leaves a
        // window -- and a crash inside it leaves the forbidden state
        // permanently. Its before-image comes from the capture, so a TOML
        // edited between planning and applying refuses like any other stale
        // precondition rather than being deleted anyway.
        for path in update.retire {
            let file = snapshot.files.get(&path).ok_or_else(|| {
                Diagnostic::without_a_fix(
                    "workspace-retire-not-captured",
                    path.to_string(),
                    format!("cannot retire `{path}`: it was not captured before planning"),
                )
            })?;
            let before = file_image(
                &file.bytes,
                if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
                blobs,
            )?;
            operations.push(PlannedOperation::RemoveFile { path, before });
        }
    }
    Ok(())
}
