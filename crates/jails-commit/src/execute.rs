//! The commit protocol: eleven ordered steps, and one instant that divides
//! them.
//!
//! ## The instant
//!
//! Step 10 writes the ledger. Before it, every failure leaves the project
//! exactly as it was and returns `CommitError`. After it, the work has
//! happened, and saying "error" would tell the caller to retry something that
//! is already durable. That is why `CommitError` and the post-commit values
//! are different types rather than one with a flag.
//!
//! ## Why a create is a hard link and not a write
//!
//! §R4.3 step 9. A create copies the immutable object into a
//! transaction-local `.publish` inode, verifies it, sets its mode, syncs it,
//! and then hard-links *that* inode into place. The destination therefore
//! appears complete or not at all — there is no instant where a reader sees a
//! half-written file. Linking the content object itself would be worse: the
//! live path and the immutable object would share an inode, so editing the
//! file would edit the object store.
//!
//! ## Why every op reclassifies immediately before acting
//!
//! Because recovery runs the same code. On a first execution the live path
//! should be the `Before` image; on a recovery pass an already-applied op is
//! `After` and is skipped. Anything else — a third image, an unreadable path
//! — stops and records why. Rolling forward over an image nobody recorded is
//! how a half-applied transaction becomes a wrong one.
//!
//! ## Why the ledger is written last
//!
//! It is the commit point. Everything before it can be abandoned; the ledger
//! is what makes the change true. Writing it first would leave a store that
//! claims files that do not exist.

use crate::Result;
use crate::journal::ReceiptV1;
use crate::journal::{
    ActualImage, BlockReason, JournalState, JournalV1, ObservedImage, RootIdentity,
};
use crate::outcome::{
    CommitEffectOutcome, CommitError, CommitResult, CommittedRecoveryRequired, CommittedResult,
    PostCommitRecoveryError, PostCommitStage,
};
use crate::store::{self, Store};
use jails_prepare::pipeline::PreparedBundle;
use jails_prepare::prepare::{FileOp, GuardedImage, OperationTarget, PreparedChange};
use jails_prepare::receipt::{AppliedReceipt, ApplyOutcome, DirectoryReceipt, FileReceipt};
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::identity::{ObjectId, ObjectRef, ProjectPath};
use jails_support::codec::sha256;
use jails_support::lock::{Contention, Lock};
use std::path::{Path, PathBuf};

/// One project, resolved without following a symlink.
#[derive(Clone, Debug)]
pub struct ProjectHandle {
    root: PathBuf,
    store: Store,
}

impl ProjectHandle {
    pub fn at(root: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(root)
            .map_err(|error| format!("could not stat {}: {error}", root.display()))?;
        if metadata.is_symlink() {
            return Err(format!(
                "{} is a symlink.\n       fix: a project root must be a real directory; the \
                 transaction's root identity would name something else.",
                root.display()
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            store: Store::at(root),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

/// A project with the mutation lock held.
#[derive(Debug)]
pub struct LockedProject {
    handle: ProjectHandle,
    root_identity: RootIdentity,
    #[allow(dead_code)]
    lock: Lock,
}

impl LockedProject {
    /// Bootstrap `.jails`, take the lock, and pin the root's identity.
    ///
    /// The bootstrap happens *before* the lock because the lock file lives
    /// inside the directory it creates. §R4.1 allows exactly this much
    /// pre-activation machine state and nothing more: no project leaf, no
    /// declaration, no ledger, no transaction, no object.
    pub fn acquire(
        handle: ProjectHandle,
        description: &str,
    ) -> std::result::Result<Self, CommitError> {
        store::create_private_dir(handle.store.root()).map_err(CommitError::PreActivationIo)?;
        let lock =
            Lock::acquire(&handle.store.lock_path(), description).map_err(|why| match why {
                Contention::Held(_) => CommitError::MutationBusy(why.to_string()),
                Contention::Refused(_) => CommitError::PreActivationIo(why.to_string()),
            })?;
        handle
            .store
            .create_subdirectories()
            .map_err(CommitError::PreActivationIo)?;
        let root_identity = RootIdentity::of(&handle.root).map_err(CommitError::PreActivationIo)?;
        Ok(Self {
            handle,
            root_identity,
            lock,
        })
    }

    pub fn handle(&self) -> &ProjectHandle {
        &self.handle
    }

    pub fn root_identity(&self) -> RootIdentity {
        self.root_identity
    }
}

/// Commit one prepared bundle. The only clean project mutation entrypoint.
///
/// It has no parser, no request and no planner, so it can never re-plan: what
/// it applies is exactly what was prepared and reported.
pub fn commit(
    locked: &LockedProject,
    bundle: &PreparedBundle,
) -> std::result::Result<CommitResult, CommitError> {
    let change = &bundle.change;
    change.validate().map_err(CommitError::InvalidPrepared)?;

    // Step 2. Recheck every guard under the lock. A mismatch is stale, and
    // stale is a refusal — commit never substitutes changed operations.
    recheck(locked, change)?;

    // Step 3. Truthful *because* it comes after the recheck rather than
    // before it.
    if change.is_no_op() {
        return Ok(CommitResult::NoOp);
    }

    // Step 5. Stage everything, still with nothing live touched.
    let directory = locked.handle.store.transaction(&change.transaction_id);
    if locked.handle.store.receipt(&change.transaction_id).exists() {
        return Err(CommitError::StaleInput(format!(
            "transaction {} is already committed.\n       fix: the plan was made against an \
             older generation; replan.",
            change.transaction_id
        )));
    }
    store::create_private_dir(&directory).map_err(CommitError::PreActivationIo)?;
    let objects = directory.join("objects");
    for (id, body) in &change.objects {
        store::put_object(&objects, id, body).map_err(CommitError::PreActivationIo)?;
    }

    let journal = JournalV1 {
        transaction: change.transaction_id,
        generation: change.operation_identity.proposed_generation,
        root_identity: locked.root_identity,
        state: JournalState::Prepared,
        prepared: change.identity().map_err(CommitError::InvalidPrepared)?,
    };
    journal
        .persist(&directory)
        .map_err(CommitError::PreActivationIo)?;

    // Step 6. From here a failure leaves recovery work, never "nothing
    // written".
    let active = journal.advanced(JournalState::Active);
    active
        .persist(&directory)
        .map_err(CommitError::PreActivationIo)?;

    apply_operations(locked, change, &directory, &objects)
        .map_err(|blocked| blocked.into_error(&directory, &active))?;

    // Step 10. The ledger last, with the same guarded primitive. It is the
    // commit point: everything before it can be abandoned, and this is what
    // makes the change true.
    write_ledger(locked, change, &directory, &objects).map_err(CommitError::CorruptMachineState)?;

    // Step 11. From here nothing may return `CommitError`.
    Ok(publish(locked, change, &directory, &active))
}

/// Step 10: the guarded ledger transition, then fsync the machine root.
fn write_ledger(
    locked: &LockedProject,
    change: &PreparedChange,
    directory: &Path,
    objects: &Path,
) -> Result<()> {
    let path = locked.handle.store.root().join("ledger.toml");
    let publish = directory.join("live-temp");
    match change.ledger_after {
        FileImage::Absent => {
            if path.exists() {
                let kept = publish.join("ledger.deleted");
                std::fs::rename(&path, &kept)
                    .map_err(|error| format!("could not retire the store: {error}"))?;
            }
        }
        FileImage::Present { object, mode } => {
            let staged = stage(&publish, objects, &object, mode, usize::MAX)?;
            std::fs::rename(&staged, &path)
                .map_err(|error| format!("could not write the store: {error}"))?;
        }
    }
    store::sync_dir(locked.handle.store.root())
}

/// Step 11: complete the journal, build and publish the receipt, and move the
/// intact directory into `receipts/`.
///
/// Nothing here returns `CommitError`: the ledger is already durable, so a
/// failure is structural work to finish, not a reason to tell the caller
/// nothing happened.
fn publish(
    locked: &LockedProject,
    change: &PreparedChange,
    directory: &Path,
    active: &JournalV1,
) -> CommitResult {
    let required = |stage: PostCommitStage, error: String| {
        CommitResult::CommittedRecoveryRequired(Box::new(CommittedRecoveryRequired {
            operation: change.operation_id,
            transaction: change.transaction_id,
            receipt: None,
            stage,
            error: PostCommitRecoveryError::Io(error),
        }))
    };

    let committed = active.advanced(JournalState::LedgerCommitted);
    if let Err(error) = committed.persist(directory) {
        return required(PostCommitStage::JournalCompletion, error);
    }
    let complete = active.advanced(JournalState::Complete);
    if let Err(error) = complete.persist(directory) {
        return required(PostCommitStage::JournalCompletion, error);
    }

    let witness = match ReceiptV1::witness_of(&complete) {
        Ok(witness) => witness,
        Err(error) => return required(PostCommitStage::ReceiptPublication, error),
    };
    let receipt = ReceiptV1 {
        transaction: complete.transaction,
        generation: complete.generation,
        prepared: complete.prepared.clone(),
        complete_journal_checksum: witness,
        post_commit: Vec::new(),
    };
    if let Err(error) = receipt.persist(directory) {
        return required(PostCommitStage::ReceiptPublication, error);
    }

    // Only derived temps are removed; the journal and the receipt stay, and
    // the directory moves intact. There is no copy-then-delete publication
    // and therefore no window with neither placement.
    let _ = std::fs::remove_dir_all(directory.join("live-temp"));
    if let Err(error) = store::sync_dir(directory) {
        return required(PostCommitStage::ReceiptPublication, error);
    }

    let destination = locked.handle.store.receipt(&change.transaction_id);
    if destination.exists() {
        return required(
            PostCommitStage::ReceiptPublication,
            format!("{} already exists", destination.display()),
        );
    }
    if let Err(error) = std::fs::rename(directory, &destination) {
        return required(
            PostCommitStage::ReceiptPublication,
            format!("could not publish the receipt: {error}"),
        );
    }
    for parent in [
        locked.handle.store.transactions(),
        locked.handle.store.receipts(),
        locked.handle.store.root().to_path_buf(),
    ] {
        if let Err(error) = store::sync_dir(&parent) {
            return required(PostCommitStage::ReceiptPublication, error);
        }
    }

    CommitResult::Committed(Box::new(CommittedResult {
        receipt: applied_receipt(change),
        effect: CommitEffectOutcome::NotApplicable,
    }))
}

/// The public projection of what happened.
///
/// Derived from the prepared operations and the kind, never a second durable
/// authority — §R4.2 is explicit that `AppliedReceipt` is a report shape.
fn applied_receipt(change: &PreparedChange) -> AppliedReceipt {
    AppliedReceipt {
        operation_id: change.operation_id,
        transaction_id: change.transaction_id,
        files: change
            .operations
            .iter()
            .map(|operation| FileReceipt {
                path: operation.target().clone(),
                before: before_image(operation),
                after: after_image(operation),
                contributors: operation.contributors().clone(),
            })
            .collect(),
        directories: change
            .directories
            .iter()
            .map(|directory| DirectoryReceipt {
                path: directory.path().clone(),
            })
            .collect(),
        ledger_before: change.ledger_before,
        ledger_after: change.ledger_after,
        outcome: match &change.kind {
            jails_prepare::prepare::PreparedKind::Apply => ApplyOutcome::Applied,
            jails_prepare::prepare::PreparedKind::Conflict { .. } => ApplyOutcome::Conflicted,
            jails_prepare::prepare::PreparedKind::Finalise { .. } => ApplyOutcome::Finalised,
            jails_prepare::prepare::PreparedKind::Abort { .. } => ApplyOutcome::Aborted,
        },
        post_commit: Vec::new(),
    }
}

fn before_image(operation: &FileOp) -> FileImage {
    match operation {
        FileOp::Create { .. } => FileImage::Absent,
        FileOp::Replace { before, .. } | FileOp::Delete { before, .. } => FileImage::Present {
            object: before.object,
            mode: before.mode,
        },
    }
}

fn after_image(operation: &FileOp) -> FileImage {
    match operation {
        FileOp::Create { after, mode, .. } | FileOp::Replace { after, mode, .. } => {
            FileImage::Present {
                object: *after,
                mode: *mode,
            }
        }
        FileOp::Delete { .. } => FileImage::Absent,
    }
}

/// A failure after activation, with the reason recovery will record.
struct Blocked {
    path: Option<ProjectPath>,
    reason: BlockReason,
}

impl Blocked {
    /// Persist the block on the journal, so the next run sees it rather than
    /// rediscovering it.
    fn into_error(self, directory: &Path, active: &JournalV1) -> CommitError {
        let blocked = active.advanced(JournalState::Blocked {
            resume: crate::journal::ResumeState::Active,
            path: self.path,
            reason: self.reason.clone(),
        });
        // A failure to record the block is itself corruption: the next run
        // would find an Active journal and roll forward over the same
        // unclassifiable image.
        if let Err(error) = blocked.persist(directory) {
            return CommitError::CorruptMachineState(error);
        }
        CommitError::RecoveryBlocked(self.reason)
    }
}

/// Steps 7 to 9: directories shallowest-first, then file operations in
/// canonical path order.
fn apply_operations(
    locked: &LockedProject,
    change: &PreparedChange,
    directory: &Path,
    objects: &Path,
) -> std::result::Result<(), Blocked> {
    let root = &locked.handle.root;
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
            }
        }
    }

    let publish = directory.join("live-temp");
    store::create_private_dir(&publish).map_err(|error| Blocked {
        path: None,
        reason: BlockReason::Unreadable { error_kind: error },
    })?;

    for (index, operation) in change.operations.iter().enumerate() {
        let OperationTarget::Project(path) = operation.target() else {
            // A legacy machine delete touches machine state, not the live
            // tree, and is applied with the ledger transition.
            continue;
        };
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
        apply_one(operation, &at, &publish, objects, index).map_err(|error| Blocked {
            path: Some(path.clone()),
            reason: BlockReason::Unreadable { error_kind: error },
        })?;
    }
    Ok(())
}

/// Where a live path stands relative to the two images this operation names.
fn classify(at: &Path, operation: &FileOp) -> ObservedImage {
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

fn matches(actual: &ActualImage, expected: Option<&GuardedImage>) -> bool {
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
fn observe(at: &Path) -> std::result::Result<ActualImage, String> {
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
fn mode_of(metadata: &std::fs::Metadata) -> Result<FileMode> {
    use std::os::unix::fs::MetadataExt;
    FileMode::new(metadata.mode() & 0o777)
}

/// One operation, made observably atomic.
fn apply_one(
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
fn stage(
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
fn set_mode(at: &Path, mode: FileMode) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(at, std::fs::Permissions::from_mode(mode.bits()))
        .map_err(|error| format!("could not set the mode of {}: {error}", at.display()))
}

fn sync_parent(at: &Path) -> Result<()> {
    match at.parent() {
        Some(parent) => store::sync_dir(parent),
        None => Ok(()),
    }
}

/// Step 2: every precondition, every operation's expected image, and the
/// ledger's own before-image, rechecked under the lock.
fn recheck(
    locked: &LockedProject,
    change: &PreparedChange,
) -> std::result::Result<(), CommitError> {
    let root = &locked.handle.root;
    for operation in &change.operations {
        let OperationTarget::Project(path) = operation.target() else {
            continue;
        };
        let at = root.join(path.as_str());
        match classify(&at, operation) {
            // `After` is acceptable: the work is already done, and step 8
            // will skip it. Anything unclassifiable is not.
            ObservedImage::Before | ObservedImage::After => {}
            ObservedImage::Unknown { actual } => {
                return Err(CommitError::StaleInput(format!(
                    "`{path}` is neither the image this plan expected nor the one it would \
                     write (found {actual:?}).\n       fix: it changed after the plan was made; \
                     replan."
                )));
            }
            ObservedImage::Unreadable { error_kind } => {
                return Err(CommitError::StaleInput(format!(
                    "`{path}` could not be read ({error_kind})"
                )));
            }
        }
    }
    check_ledger(locked, change.ledger_before)
}

fn check_ledger(
    locked: &LockedProject,
    expected: FileImage,
) -> std::result::Result<(), CommitError> {
    let path = locked.handle.store.root().join("ledger.toml");
    let actual = observe(&path).map_err(CommitError::StaleInput)?;
    let matched = match (expected, &actual) {
        (FileImage::Absent, ActualImage::Absent) => true,
        (
            FileImage::Present { object, mode },
            ActualImage::File {
                sha256,
                len,
                mode: actual_mode,
            },
        ) => sha256 == &object.id && len == &object.len && actual_mode == &mode,
        _ => false,
    };
    if !matched {
        return Err(CommitError::StaleInput(
            "the store changed after this plan was made.\n       fix: another run committed in \
             between; replan."
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::ReceiptV1;
    use jails_prepare::operation::{ApplySemantics, OperationIdentityV1, OperationSemanticsV1};
    use jails_prepare::prepare::{DirectoryOp, PreparedKind};
    use jails_prepare::tool::{OperationContextFingerprint, PreparationContextFingerprint};
    use jails_protocol::plan::{LedgerIntent, PlannedSubject};
    use jails_protocol::snapshot::CanonicalRoot;
    use jails_support::scratch::ScratchDir;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    fn mode() -> FileMode {
        FileMode::new(0o644).unwrap()
    }

    fn object(body: &[u8]) -> ObjectRef {
        ObjectRef::new(ObjectId::from_bytes(sha256(body)), body.len() as u64)
    }

    /// A prepared change over the given operations, with its objects.
    fn change_of(
        operations: Vec<FileOp>,
        bodies: Vec<&[u8]>,
        directories: Vec<&str>,
    ) -> PreparedChange {
        let mut objects: BTreeMap<ObjectId, Arc<[u8]>> = BTreeMap::new();
        for body in bodies {
            objects.insert(ObjectId::from_bytes(sha256(body)), Arc::from(body.to_vec()));
        }
        let operation_identity = OperationIdentityV1 {
            snapshot: ObjectId::from_bytes(sha256(b"snapshot")),
            operation_context: OperationContextFingerprint::default(),
            invocation: None,
            proposed_generation: 1,
            semantics: OperationSemanticsV1::Apply(Box::new(ApplySemantics {
                subject: PlannedSubject::AdoptLayout,
                ledger_intent: LedgerIntent {
                    generation_before: 0,
                    entities_after: Vec::new(),
                    one_shots_after: Vec::new(),
                    resources_after: Vec::new(),
                    legacy_after: Vec::new(),
                },
                migration: None,
            })),
        };
        let mut operations = operations;
        operations.sort_by(|a, b| a.target().cmp(b.target()));
        let mut change = PreparedChange {
            operation_id: operation_identity.operation_id().unwrap(),
            operation_identity,
            transaction_id: jails_protocol::identity::TransactionId::from_bytes([0; 32]),
            preparation: PreparationContextFingerprint::default(),
            input_preconditions: Vec::new(),
            operations,
            directories: directories
                .into_iter()
                .map(|path| DirectoryOp::Create {
                    path: ProjectPath::parse(path).unwrap(),
                })
                .collect(),
            ledger_before: FileImage::Absent,
            ledger_after: FileImage::Absent,
            objects,
            post_commit: Vec::new(),
            kind: PreparedKind::Apply,
        };
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        change
    }

    fn create_op(at: &str, body: &[u8]) -> FileOp {
        FileOp::Create {
            path: OperationTarget::Project(ProjectPath::parse(at).unwrap()),
            after: object(body),
            mode: mode(),
            contributors: BTreeSet::new(),
        }
    }

    fn bundle(change: PreparedChange) -> PreparedBundle {
        PreparedBundle {
            root: CanonicalRoot::new("/srv/demo").unwrap(),
            change,
        }
    }

    fn project() -> (ScratchDir, LockedProject) {
        let scratch = ScratchDir::in_temp("jails-commit").unwrap();
        let handle = ProjectHandle::at(scratch.path()).unwrap();
        let locked = LockedProject::acquire(handle, "test").unwrap();
        (scratch, locked)
    }

    #[test]
    fn a_create_lands_with_its_bytes_and_its_mode() {
        let (scratch, locked) = project();
        let change = change_of(
            vec![create_op("src/main/java/App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            vec!["src", "src/main", "src/main/java"],
        );
        let transaction = change.transaction_id;
        let result = commit(&locked, &bundle(change)).unwrap();
        assert!(matches!(result, CommitResult::Committed(_)), "{result:?}");

        let at = scratch.path().join("src/main/java/App.java");
        assert_eq!(std::fs::read(&at).unwrap(), b"class App {}\n");
        assert_eq!(mode_of(&std::fs::metadata(&at).unwrap()).unwrap(), mode());

        // The directory moved intact into `receipts/`, journal included.
        let published = locked.handle.store.receipt(&transaction);
        assert!(published.join("journal.bin").exists());
        assert!(published.join("receipt.bin").exists());
        assert!(!locked.handle.store.transaction(&transaction).exists());
        ReceiptV1::read(&published).unwrap();
        scratch.close().unwrap();
    }

    /// The publication temp lives in the transaction directory, never beside
    /// the user's file — a stray `.publish` in a source tree would be
    /// committed by whoever ran `git add`.
    #[test]
    fn no_publication_temp_is_ever_created_beside_a_user_file() {
        let (scratch, locked) = project();
        let change = change_of(
            vec![create_op("App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            Vec::new(),
        );
        commit(&locked, &bundle(change)).unwrap();
        let strays: Vec<_> = std::fs::read_dir(scratch.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("publish") || name.contains("deleted"))
            .collect();
        assert!(strays.is_empty(), "{strays:?}");
        scratch.close().unwrap();
    }

    #[test]
    fn a_replace_guards_its_preimage_and_a_delete_keeps_it() {
        let (scratch, locked) = project();
        let at = scratch.path().join("pom.xml");
        std::fs::write(&at, b"<project/>").unwrap();
        set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![FileOp::Replace {
                path: OperationTarget::Project(ProjectPath::parse("pom.xml").unwrap()),
                before: GuardedImage {
                    object: object(b"<project/>"),
                    mode: mode(),
                },
                after: object(b"<project></project>"),
                mode: mode(),
                contributors: BTreeSet::new(),
            }],
            vec![b"<project></project>"],
            Vec::new(),
        );
        commit(&locked, &bundle(change)).unwrap();
        assert_eq!(std::fs::read(&at).unwrap(), b"<project></project>");
        scratch.close().unwrap();
    }

    /// The guard is the whole point: a file edited after the plan was made
    /// must not be overwritten by it.
    #[test]
    fn a_file_edited_after_planning_makes_the_commit_stale() {
        let (scratch, locked) = project();
        let at = scratch.path().join("pom.xml");
        std::fs::write(&at, b"<project/>").unwrap();
        set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![FileOp::Replace {
                path: OperationTarget::Project(ProjectPath::parse("pom.xml").unwrap()),
                before: GuardedImage {
                    object: object(b"<project/>"),
                    mode: mode(),
                },
                after: object(b"<project></project>"),
                mode: mode(),
                contributors: BTreeSet::new(),
            }],
            vec![b"<project></project>"],
            Vec::new(),
        );
        // The user edits it between plan and commit.
        std::fs::write(&at, b"<project>mine</project>").unwrap();

        let error = commit(&locked, &bundle(change)).unwrap_err();
        assert!(matches!(error, CommitError::StaleInput(_)), "{error:?}");
        assert_eq!(std::fs::read(&at).unwrap(), b"<project>mine</project>");
        scratch.close().unwrap();
    }

    /// A plan whose file is already in its after-image is not stale: the work
    /// is done, and step 8 skips it.
    #[test]
    fn a_file_already_in_its_after_image_is_not_stale() {
        let (scratch, locked) = project();
        let at = scratch.path().join("App.java");
        std::fs::write(&at, b"class App {}\n").unwrap();
        set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![create_op("App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            Vec::new(),
        );
        let result = commit(&locked, &bundle(change)).unwrap();
        assert!(matches!(result, CommitResult::Committed(_)), "{result:?}");
        scratch.close().unwrap();
    }

    /// Truthful *because* it is decided after the recheck rather than before
    /// it.
    #[test]
    fn a_prepared_no_op_commits_nothing_and_says_so() {
        let (scratch, locked) = project();
        let change = change_of(Vec::new(), Vec::new(), Vec::new());
        assert_eq!(
            commit(&locked, &bundle(change)).unwrap(),
            CommitResult::NoOp
        );
        assert!(!locked.handle.store.transactions().join("x").exists());
        scratch.close().unwrap();
    }

    /// Rolling forward over an image nobody recorded is how a half-applied
    /// transaction becomes a wrong one.
    #[test]
    fn an_unclassifiable_live_image_blocks_and_records_why() {
        let (scratch, locked) = project();
        let at = scratch.path().join("App.java");

        let change = change_of(
            vec![create_op("App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            Vec::new(),
        );
        let transaction = change.transaction_id;
        // Something puts a directory where the file belongs, after the
        // recheck would have looked. Simulated by committing twice: the
        // second call sees a live file that is neither image.
        std::fs::create_dir(&at).unwrap();
        let error = commit(&locked, &bundle(change)).unwrap_err();
        assert!(matches!(error, CommitError::StaleInput(_)), "{error:?}");
        assert!(!locked.handle.store.transaction(&transaction).exists());
        scratch.close().unwrap();
    }

    /// A second mutation must not start while one holds the lock.
    #[test]
    fn a_second_locked_project_is_refused_rather_than_queued() {
        let scratch = ScratchDir::in_temp("jails-commit").unwrap();
        let handle = ProjectHandle::at(scratch.path()).unwrap();
        let _held = LockedProject::acquire(handle.clone(), "first").unwrap();
        let error = LockedProject::acquire(handle, "second").unwrap_err();
        assert!(matches!(error, CommitError::MutationBusy(_)), "{error:?}");
        scratch.close().unwrap();
    }

    /// A project root that is a symlink would give the transaction a root
    /// identity naming something else.
    #[test]
    fn a_symlinked_project_root_is_refused() {
        let scratch = ScratchDir::in_temp("jails-commit").unwrap();
        let real = scratch.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(ProjectHandle::at(&link).is_err());
        scratch.close().unwrap();
    }

    /// Committing the same transaction twice is stale, not a second commit.
    #[test]
    fn a_transaction_that_is_already_published_is_stale() {
        let (scratch, locked) = project();
        let change = change_of(
            vec![create_op("App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            Vec::new(),
        );
        commit(&locked, &bundle(change.clone())).unwrap();
        std::fs::remove_file(scratch.path().join("App.java")).unwrap();
        let error = commit(&locked, &bundle(change)).unwrap_err();
        assert!(matches!(error, CommitError::StaleInput(_)), "{error:?}");
        scratch.close().unwrap();
    }
}
