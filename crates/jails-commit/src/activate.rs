//! Putting bytes where the plan says, and looking at what is there.
//!
//! Split from `execute.rs` by secret. That module owns the *protocol* -- the
//! ordered steps of a commit, what is guarded before the ledger rename and
//! what is unwound after it -- and this one owns the mechanics those steps are
//! made of: observing a path, staging bytes beside their destination,
//! publishing them, and saying whether what was found is what was expected.
//!
//! The distinction is not cosmetic. The protocol changes when the transaction
//! model changes; these change when the filesystem does -- a new mode bit, a
//! platform without hard links, a different way to make a rename durable.
//! They had no reason to change together and every reason to be read apart.

use std::path::{Path, PathBuf};

use jails_prepare::prepare::{FileOp, GuardedImage, PreparedIdentityV1};
use jails_protocol::conflict::FileMode;
use jails_protocol::identity::{ObjectId, ObjectRef, ProjectPath};

use crate::Result;
use crate::execute::{Blocked, LockedProject};
use crate::journal::{ActualImage, BlockReason, ObservedImage};
use crate::store;
use jails_support::codec::sha256;

/// Steps 7 to 9: directories shallowest-first, then file operations in
/// canonical path order.
pub(crate) fn apply_operations(
    locked: &LockedProject,
    change: &PreparedIdentityV1,
    directory: &Path,
    objects: &Path,
) -> std::result::Result<(), Blocked> {
    let root = locked.root();
    let mut directories: Vec<&ProjectPath> =
        change.directories.iter().map(|one| one.path()).collect();
    directories.sort_by_key(|path| path.as_str().matches('/').count());
    for path in directories {
        let at = root.join(path.as_str());
        match std::fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.is_dir() => {
                // On a recovery pass an ordinary directory is the permitted
                // after-state. Anything else is not.
            }
            Ok(_) => {
                return Err(Blocked {
                    path: Some(path.clone()),
                    reason: BlockReason::UnknownLiveImage {
                        actual: ActualImage::Other,
                    },
                });
            }
            Err(_) => {
                std::fs::create_dir(&at).map_err(|error| Blocked {
                    path: Some(path.clone()),
                    reason: BlockReason::Unreadable {
                        error_kind: error.kind().to_string(),
                    },
                })?;
                if let Some(parent) = at.parent() {
                    let _ = store::sync_dir(parent);
                }
                crate::fault::trip("after-directory-sync").map_err(|error| Blocked {
                    path: Some(path.clone()),
                    reason: BlockReason::Unreadable { error_kind: error },
                })?;
            }
        }
    }

    let publish = directory.join("live-temp");
    store::create_private_dir(&publish)
        .and_then(|()| crate::fault::trip("after-live-temp-sync"))
        .map_err(|error| Blocked {
            path: None,
            reason: BlockReason::Unreadable { error_kind: error },
        })?;

    for (index, operation) in change.operations.iter().enumerate() {
        let path = operation.target();
        let at = root.join(path.as_str());
        match classify(&at, operation) {
            ObservedImage::After => continue,
            ObservedImage::Before => {}
            ObservedImage::Unknown { actual } => {
                return Err(Blocked {
                    path: Some(path.clone()),
                    reason: BlockReason::UnknownLiveImage { actual },
                });
            }
            ObservedImage::Unreadable { error_kind } => {
                return Err(Blocked {
                    path: Some(path.clone()),
                    reason: BlockReason::Unreadable { error_kind },
                });
            }
        }
        // Missing or wrong bytes are machine-state corruption, and naming the
        // object is what makes that actionable — "could not read a file
        // under .jails" is not.
        if let Some(after) = operation.after()
            && store::read_object(objects, &after.id).is_err()
        {
            return Err(Blocked {
                path: Some(path.clone()),
                reason: BlockReason::CorruptObject(after.id),
            });
        }
        crate::fault::trip("before-file").map_err(|error| Blocked {
            path: Some(path.clone()),
            reason: BlockReason::Unreadable { error_kind: error },
        })?;
        apply_one(operation, &at, &publish, objects, index).map_err(|error| Blocked {
            path: Some(path.clone()),
            reason: BlockReason::Unreadable { error_kind: error },
        })?;
        crate::fault::trip("after-file-dirsync").map_err(|error| Blocked {
            path: Some(path.clone()),
            reason: BlockReason::Unreadable { error_kind: error },
        })?;
    }
    Ok(())
}

/// Where a live path stands relative to the two images this operation names.
pub(crate) fn classify(at: &Path, operation: &FileOp) -> ObservedImage {
    let (before, after) = match operation {
        FileOp::Create { after, mode, .. } => (
            None,
            Some(GuardedImage {
                object: *after,
                mode: *mode,
            }),
        ),
        FileOp::Replace {
            before,
            after,
            mode,
            ..
        } => (
            Some(*before),
            Some(GuardedImage {
                object: *after,
                mode: *mode,
            }),
        ),
        FileOp::Delete { before, .. } => (Some(*before), None),
    };
    let actual = match observe(at) {
        Ok(actual) => actual,
        Err(error_kind) => return ObservedImage::Unreadable { error_kind },
    };
    if matches(&actual, before.as_ref()) {
        return ObservedImage::Before;
    }
    if matches(&actual, after.as_ref()) {
        return ObservedImage::After;
    }
    ObservedImage::Unknown { actual }
}

pub(crate) fn matches(actual: &ActualImage, expected: Option<&GuardedImage>) -> bool {
    match (actual, expected) {
        (ActualImage::Absent, None) => true,
        (
            ActualImage::File { sha256, len, mode },
            Some(GuardedImage {
                object,
                mode: expected_mode,
            }),
        ) => sha256 == &object.id && len == &object.len && mode == expected_mode,
        _ => false,
    }
}

/// What is actually there. A symlink or a directory is its own answer, never
/// followed: a plan that named a file must not act on something else.
pub(crate) fn observe(at: &Path) -> std::result::Result<ActualImage, String> {
    let metadata = match std::fs::symlink_metadata(at) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ActualImage::Absent);
        }
        Err(error) => return Err(error.kind().to_string()),
    };
    if metadata.is_symlink() {
        return Ok(ActualImage::Symlink);
    }
    if metadata.is_dir() {
        return Ok(ActualImage::Directory);
    }
    if !metadata.is_file() {
        return Ok(ActualImage::Other);
    }
    let bytes = std::fs::read(at).map_err(|error| error.kind().to_string())?;
    Ok(ActualImage::File {
        sha256: ObjectId::from_bytes(sha256(&bytes)),
        len: bytes.len() as u64,
        mode: mode_of(&metadata).map_err(|error| error.to_string())?,
    })
}

#[cfg(unix)]
pub(crate) fn mode_of(metadata: &std::fs::Metadata) -> Result<FileMode> {
    use std::os::unix::fs::MetadataExt;
    FileMode::new(metadata.mode() & 0o777)
}

/// One operation, made observably atomic.
pub(crate) fn apply_one(
    operation: &FileOp,
    at: &Path,
    publish: &Path,
    objects: &Path,
    index: usize,
) -> Result<()> {
    match operation {
        FileOp::Create { after, mode, .. } => {
            let staged = stage(publish, objects, after, *mode, index)?;
            // A hard link, so the destination appears complete or not at all.
            // Linking the content object itself would make the live file and
            // the immutable object share an inode.
            std::fs::hard_link(&staged, at)
                .map_err(|error| format!("could not publish {}: {error}", at.display()))?;
            sync_parent(at)
        }
        FileOp::Replace { after, mode, .. } => {
            let staged = stage(publish, objects, after, *mode, index)?;
            std::fs::rename(&staged, at)
                .map_err(|error| format!("could not replace {}: {error}", at.display()))?;
            sync_parent(at)
        }
        FileOp::Delete { .. } => {
            // Renamed rather than unlinked, so the preimage survives for an
            // abort and for audit.
            let kept = publish.join(format!("{index}.deleted"));
            std::fs::rename(at, &kept)
                .map_err(|error| format!("could not remove {}: {error}", at.display()))?;
            let _ = store::sync_dir(publish);
            sync_parent(at)
        }
    }
}

/// Copy an object into its own publication inode, verify it, set the mode.
pub(crate) fn stage(
    publish: &Path,
    objects: &Path,
    object: &ObjectRef,
    mode: FileMode,
    index: usize,
) -> Result<PathBuf> {
    let bytes = store::read_object(objects, &object.id)?;
    if bytes.len() as u64 != object.len {
        return Err(format!(
            "object {} is {} bytes and the plan says {}",
            object.id,
            bytes.len(),
            object.len
        ));
    }
    let staged = publish.join(format!("{index}.publish"));
    let _ = std::fs::remove_file(&staged);
    {
        use std::io::Write;
        let mut file = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| format!("could not create {}: {error}", staged.display()))?;
        file.write_all(&bytes)
            .map_err(|error| format!("could not write {}: {error}", staged.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not sync {}: {error}", staged.display()))?;
    }
    set_mode(&staged, mode)?;
    let written = std::fs::read(&staged)
        .map_err(|error| format!("could not reread {}: {error}", staged.display()))?;
    if ObjectId::from_bytes(sha256(&written)) != object.id {
        return Err(format!(
            "{} does not hold the bytes it was staged from",
            staged.display()
        ));
    }
    Ok(staged)
}

#[cfg(unix)]
pub(crate) fn set_mode(at: &Path, mode: FileMode) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(at, std::fs::Permissions::from_mode(mode.bits()))
        .map_err(|error| format!("could not set the mode of {}: {error}", at.display()))
}

pub(crate) fn sync_parent(at: &Path) -> Result<()> {
    match at.parent() {
        Some(parent) => store::sync_dir(parent),
        None => Ok(()),
    }
}
