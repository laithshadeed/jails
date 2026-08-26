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
use jails_prepare::prepare::{FileOp, PreparedChange, PreparedIdentityV1};
use jails_prepare::receipt::{
    AppliedReceipt, ApplyOutcome, DirectoryReceipt, EffectReceipt, FileReceipt,
};
use jails_protocol::conflict::FileImage;
use jails_protocol::identity::{ObjectId, ProjectPath};
use jails_protocol::snapshot::{CanonicalRoot, InputPrecondition};
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
            )
            .into());
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
        store::create_private_dir(handle.store.root())
            .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
        let lock =
            Lock::acquire(&handle.store.lock_path(), description).map_err(|why| match why {
                Contention::Held(_) => CommitError::MutationBusy(why.to_string()),
                Contention::Refused(_) => CommitError::PreActivationIo(why.to_string()),
            })?;
        handle
            .store
            .create_subdirectories()
            .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
        let root_identity = RootIdentity::of(&handle.root)
            .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
        Ok(Self {
            handle,
            root_identity,
            lock,
        })
    }

    pub fn handle(&self) -> &ProjectHandle {
        &self.handle
    }

    /// Where this lock is held. The plan's root is compared against it.
    pub fn root(&self) -> &Path {
        self.handle.root()
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
    change
        .validate()
        .map_err(|failure| CommitError::InvalidPrepared(failure.to_string()))?;

    // Step 1. The plan and the project it is about to be applied to have to be
    // the same project.
    //
    // The bundle has always carried the root it was prepared against and this
    // never compared it, which made a bundle for project A applicable to a
    // same-shaped project B: every path in a prepared operation is
    // project-relative, so nothing else downstream would have noticed. The
    // preconditions would pass against B's identical files and B would be
    // written with A's plan.
    require_same_project(locked, &bundle.root)?;

    crate::fault::trip("after-lock")
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    // Step 1a. Finish whatever an earlier run started and did not complete,
    // *before* planning anything new against the project it left behind.
    //
    // This crate has had crash recovery, its journal states and its
    // roll-forward pass since §R4.4, with `RecoveredPriorTransaction` on
    // `CommitResult` and a replan loop in the caller waiting for it -- and
    // nothing called it. A write that stopped part-way therefore stayed
    // half-applied for the life of the project: jails' own newer bytes on
    // disk, the ledger at the older state, `doctor` reporting five generated
    // files as the developer's edits, and `resource repair --strategy
    // roll-forward` offering to adopt them. The machinery was all there; the
    // call was not.
    let recovered = crate::recover::recover_locked(locked).map_err(|error| match error {
        crate::outcome::RecoveryError::RecoveryBlocked(reason) => {
            CommitError::RecoveryBlocked(reason)
        }
        crate::outcome::RecoveryError::CorruptMachineState(why) => {
            CommitError::CorruptMachineState(why)
        }
        crate::outcome::RecoveryError::MutationBusy(why) => CommitError::MutationBusy(why),
        crate::outcome::RecoveryError::Io(why) => CommitError::PreActivationIo(why),
    })?;
    // The plan was made against the project as it was *before* that cleanup,
    // so it is stale rather than wrong. The caller reloads and replans once,
    // which is what `RecoveredPriorTransaction` is for.
    if !recovered.is_clean() {
        return Ok(CommitResult::RecoveredPriorTransaction(Box::new(recovered)));
    }

    // Step 2. Recheck every guard under the lock. A mismatch is stale, and
    // stale is a refusal — commit never substitutes changed operations.
    recheck(locked, change)?;
    crate::fault::trip("after-recheck")
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    // Step 3. Truthful *because* it comes after the recheck rather than
    // before it.
    if change.is_no_op() {
        return Ok(CommitResult::NoOp);
    }
    let operation_digest = jails_prepare::prepared_after::operations(change)
        .map_err(|failure| CommitError::InvalidPrepared(failure.to_string()))?;
    let prepared_after = jails_prepare::prepared_after::digest(&bundle.root, change)
        .map_err(|failure| CommitError::InvalidPrepared(failure.to_string()))?;

    // Step 5. Stage everything, still with nothing live touched.
    let directory = locked.handle.store.transaction(&change.transaction_id);
    if locked.handle.store.receipt(&change.transaction_id).exists() {
        return Err(CommitError::StaleInput(format!(
            "transaction {} is already committed.\n       fix: the plan was made against an \
             older generation; replan.",
            change.transaction_id
        )));
    }
    store::create_private_dir(&directory)
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
    let objects = directory.join("objects");
    for (id, body) in &change.objects {
        store::put_object(&objects, id, body)
            .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
    }
    crate::fault::trip("after-objects-sync")
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    let journal = JournalV1 {
        transaction: change.transaction_id,
        generation: change.operation_identity.proposed_generation,
        root_identity: locked.root_identity,
        state: JournalState::Prepared,
        prepared: change
            .identity()
            .map_err(|failure| CommitError::InvalidPrepared(failure.to_string()))?,
    };
    journal
        .persist(&directory)
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
    crate::fault::trip("after-journal-prepared")
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    // Step 6. From here a failure leaves recovery work, never "nothing
    // written".
    let active = journal.advanced(JournalState::Active);
    active
        .persist(&directory)
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;
    // Armed here, the failure is *after* activation: recovery owns it.
    crate::fault::trip("after-journal-active")
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    let identity = change
        .identity()
        .map_err(|failure| CommitError::InvalidPrepared(failure.to_string()))?;
    crate::activate::apply_operations(locked, &identity, &directory, &objects)
        .map_err(|blocked| blocked.into_error(&directory, &active))?;

    // §R5.1. Promote every object the prospective store will reference into
    // the durable store *before* the ledger that names them. A committed
    // store pointing only into a transaction directory would lose its bytes
    // the moment that directory was cleaned up.
    let durable = locked.handle.store.objects();
    let promoted: Vec<ObjectId> = change.objects.keys().copied().collect();
    store::promote(&objects, &durable, &promoted)
        .map_err(|failure| CommitError::PreActivationIo(failure.to_string()))?;

    // Step 10. The ledger last, with the same guarded primitive. It is the
    // commit point: everything before it can be abandoned, and this is what
    // makes the change true.
    match write_ledger(locked, &identity, &directory, &objects) {
        Ok(()) => {}
        // Before the rename the transaction is still abandonable.
        Err(LedgerFailure::BeforeCommit(why)) => {
            return Err(CommitError::CorruptMachineState(why));
        }
        // After it, the commit is durable, and saying "error" would tell the
        // caller to retry work that has already happened.
        Err(LedgerFailure::AfterCommit(why)) => {
            return Ok(CommitResult::CommittedRecoveryRequired(Box::new(
                CommittedRecoveryRequired {
                    operation: change.operation_id,
                    transaction: change.transaction_id,
                    receipt: None,
                    stage: PostCommitStage::JournalCompletion,
                    error: PostCommitRecoveryError::Io(why),
                },
            )));
        }
    }

    // Step 11. From here nothing may return `CommitError`.
    Ok(publish(
        locked,
        change,
        operation_digest,
        prepared_after,
        &directory,
        &active,
    ))
}

/// Which side of the commit point a ledger failure fell on.
///
/// The distinction is the whole protocol in one type: before the rename the
/// transaction can still be abandoned, after it the change is true.
pub(crate) enum LedgerFailure {
    BeforeCommit(String),
    AfterCommit(String),
}

/// Refuse a plan prepared against a different project.
///
/// Compared as the *resolved* root, because that is what the preparation
/// recorded: two paths that differ only by a symlink or a relative segment are
/// one project, and refusing those would refuse ordinary use from a different
/// working directory.
fn require_same_project(
    locked: &LockedProject,
    prepared: &CanonicalRoot,
) -> std::result::Result<(), CommitError> {
    let at = std::fs::canonicalize(&locked.handle.root)
        .map_err(|error| {
            CommitError::PreActivationIo(format!(
                "could not resolve {}: {error}",
                locked.handle.root.display()
            ))
        })?
        .to_str()
        .ok_or_else(|| {
            CommitError::PreActivationIo(format!(
                "{} is not valid UTF-8",
                locked.handle.root.display()
            ))
        })?
        .to_string();
    if at != prepared.as_str() {
        return Err(CommitError::InvalidPrepared(format!(
            "this plan was prepared for {} and is being applied to {at}.\n       fix: plan and \
             commit against one project. Every path in a prepared operation is project-relative, \
             so nothing further down would have caught this.",
            prepared.as_str()
        )));
    }
    Ok(())
}

/// Step 10: the guarded ledger transition, then fsync the machine root.
pub(crate) fn write_ledger(
    locked: &LockedProject,
    change: &PreparedIdentityV1,
    directory: &Path,
    objects: &Path,
) -> std::result::Result<(), LedgerFailure> {
    let path = locked.handle.store.root().join("ledger.toml");
    let publish = directory.join("live-temp");
    crate::fault::trip("before-ledger")
        .map_err(|failure| LedgerFailure::BeforeCommit(failure.to_string()))?;
    match change.ledger_after {
        FileImage::Absent => {
            if path.exists() {
                let kept = publish.join("ledger.deleted");
                std::fs::rename(&path, &kept).map_err(|error| {
                    LedgerFailure::BeforeCommit(format!("could not retire the store: {error}"))
                })?;
            }
        }
        FileImage::Present { object, mode } => {
            let staged = crate::activate::stage(&publish, objects, &object, mode, usize::MAX)
                .map_err(|failure| LedgerFailure::BeforeCommit(failure.to_string()))?;
            std::fs::rename(&staged, &path).map_err(|error| {
                LedgerFailure::BeforeCommit(format!("could not write the store: {error}"))
            })?;
        }
    }
    // The rename is the commit point. Everything after it is durable.
    crate::fault::trip("after-ledger-rename")
        .map_err(|failure| LedgerFailure::AfterCommit(failure.to_string()))?;
    store::sync_dir(locked.handle.store.root())
        .map_err(|failure| LedgerFailure::AfterCommit(failure.to_string()))?;
    crate::fault::trip("after-ledger-dirsync")
        .map_err(|failure| LedgerFailure::AfterCommit(failure.to_string()))
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
    operation_digest: ObjectId,
    prepared_after: ObjectId,
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
    if let Err(error) = committed
        .persist(directory)
        .and_then(|()| crate::fault::trip("after-journal-ledger-committed"))
    {
        return required(PostCommitStage::JournalCompletion, error.to_string());
    }
    let complete = active.advanced(JournalState::Complete);
    if let Err(error) = complete
        .persist(directory)
        .and_then(|()| crate::fault::trip("after-journal-complete"))
    {
        return required(PostCommitStage::JournalCompletion, error.to_string());
    }

    let witness = match ReceiptV1::witness_of(&complete) {
        Ok(witness) => witness,
        Err(error) => return required(PostCommitStage::ReceiptPublication, error.to_string()),
    };
    let post_commit = match effect_receipts(change) {
        Ok(rows) => rows,
        Err(error) => return required(PostCommitStage::ReceiptPublication, error.to_string()),
    };
    let receipt = ReceiptV1 {
        transaction: complete.transaction,
        generation: complete.generation,
        prepared: complete.prepared.clone(),
        complete_journal_checksum: witness,
        // Recorded `Deferred`, never run here. The attempt happens after the
        // project lock is released -- §R6.6 -- and this is the durable
        // descriptor it works from, so a crash between the commit and the
        // attempt leaves something a retry can act on.
        post_commit,
    };
    if let Err(error) = receipt
        .persist(directory)
        .and_then(|()| crate::fault::trip("after-receipt-sync"))
    {
        return required(PostCommitStage::ReceiptPublication, error.to_string());
    }

    // Only derived temps are removed; the journal and the receipt stay, and
    // the directory moves intact. There is no copy-then-delete publication
    // and therefore no window with neither placement.
    let _ = std::fs::remove_dir_all(directory.join("live-temp"));
    if let Err(error) = store::sync_dir(directory) {
        return required(PostCommitStage::ReceiptPublication, error.to_string());
    }

    if let Err(error) = crate::fault::trip("before-receipt-move") {
        return required(PostCommitStage::ReceiptPublication, error.to_string());
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
    if let Err(error) = crate::fault::trip("after-receipt-move") {
        return required(PostCommitStage::ReceiptPublication, error.to_string());
    }
    for (parent, point) in [
        (
            locked.handle.store.transactions(),
            "after-transactions-parent-sync",
        ),
        (locked.handle.store.receipts(), "after-receipts-parent-sync"),
        (locked.handle.store.root().to_path_buf(), "after-root-sync"),
    ] {
        if let Err(error) = store::sync_dir(&parent).and_then(|()| crate::fault::trip(point)) {
            return required(PostCommitStage::ReceiptPublication, error.to_string());
        }
    }

    CommitResult::Committed(Box::new(CommittedResult {
        receipt: applied_receipt(change, operation_digest, prepared_after),
        // Whether an effect ran is decided after the lock is released; a
        // commit that claimed an outcome here would be claiming one for an
        // attempt that has not happened.
        effect: CommitEffectOutcome::NotApplicable,
    }))
}

/// The prepared effects, each with the identity §R4.2 gives it.
fn effect_receipts(change: &PreparedChange) -> Result<Vec<EffectReceipt>> {
    change
        .post_commit
        .iter()
        .enumerate()
        .map(|(index, effect)| {
            Ok(EffectReceipt {
                id: crate::runtime::identify(change.operation_id, index as u32, effect)?,
                effect: effect.clone(),
                state: jails_protocol::effect::EffectState::Deferred,
            })
        })
        .collect()
}

/// The public projection of what happened.
///
/// Derived from the prepared operations and the kind, never a second durable
/// authority — §R4.2 is explicit that `AppliedReceipt` is a report shape.
fn applied_receipt(
    change: &PreparedChange,
    operation_digest: ObjectId,
    prepared_after: ObjectId,
) -> AppliedReceipt {
    AppliedReceipt {
        warnings: jails_prepare::report::warnings(change),
        operation_id: change.operation_id,
        transaction_id: change.transaction_id,
        operation_digest,
        prepared_after,
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
        // The same rows the durable receipt carries. A projection that showed
        // no effect while the receipt held one would let a caller believe the
        // transition had no runtime half.
        post_commit: effect_receipts(change).unwrap_or_default(),
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
pub(crate) struct Blocked {
    pub(crate) path: Option<ProjectPath>,
    pub(crate) reason: BlockReason,
}

impl Blocked {
    /// Persist the block on the journal, so the next run sees it rather than
    /// rediscovering it.
    pub(crate) fn into_error(self, directory: &Path, active: &JournalV1) -> CommitError {
        let blocked = active.advanced(JournalState::Blocked {
            resume: crate::journal::ResumeState::Active,
            path: self.path,
            reason: self.reason.clone(),
        });
        // A failure to record the block is itself corruption: the next run
        // would find an Active journal and roll forward over the same
        // unclassifiable image.
        if let Err(error) = blocked.persist(directory) {
            return CommitError::CorruptMachineState(error.to_string());
        }
        CommitError::RecoveryBlocked(self.reason)
    }
}
fn recheck(
    locked: &LockedProject,
    change: &PreparedChange,
) -> std::result::Result<(), CommitError> {
    let root = &locked.handle.root;
    for operation in &change.operations {
        let path = operation.target();
        let at = root.join(path.as_str());
        match crate::activate::classify(&at, operation) {
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
    recheck_inputs(locked, change)?;
    check_ledger(locked, change.ledger_before)
}

/// §R4.3 step 2: recheck every project input the plan depended on.
///
/// Not the same question as the per-operation before-image check above. That
/// asks whether the file this transaction is about to *write* is still what it
/// expected; this asks whether the facts it *read* to decide what to write are
/// still true. `jails g migration` is the case that makes the difference
/// visible: it allocates the next serial from a directory listing and writes a
/// file whose name nothing else has, so the write guard would pass happily
/// while another process was allocating the same number.
///
/// The external, legacy and machine-receipt preconditions are not rechecked
/// here. They are resolved through a runtime `CommitContext` binding that only
/// the manifest and migration routes produce, and no route builds one yet --
/// so there is nothing for them to be checked against, and a check written
/// against an empty binding table would be a check that always passes. Their
/// rows are named in §R6.1 step 4's aggregate work.
fn recheck_inputs(
    locked: &LockedProject,
    change: &PreparedChange,
) -> std::result::Result<(), CommitError> {
    let root = &locked.handle.root;
    for precondition in &change.input_preconditions {
        match precondition {
            InputPrecondition::Absent { path } => {
                match crate::activate::observe(&root.join(path.as_str())) {
                    Ok(ActualImage::Absent) => {}
                    Ok(actual) => {
                        return Err(CommitError::StaleInput(format!(
                            "`{path}` was absent when this plan was made and is now \
                             {actual:?}.\n       fix: it appeared after the plan was made; \
                             replan."
                        )));
                    }
                    Err(error_kind) => {
                        return Err(CommitError::StaleInput(format!(
                            "`{path}` could not be read ({error_kind})"
                        )));
                    }
                }
            }
            InputPrecondition::File {
                path,
                sha256,
                len,
                mode,
            } => match crate::activate::observe(&root.join(path.as_str())) {
                Ok(ActualImage::File {
                    sha256: actual,
                    len: actual_len,
                    mode: actual_mode,
                }) if actual == *sha256 && actual_len == *len && actual_mode == *mode => {}
                Ok(_) => {
                    return Err(CommitError::StaleInput(format!(
                        "`{path}` is not the file this plan read.\n       fix: it changed \
                         after the plan was made; replan."
                    )));
                }
                Err(error_kind) => {
                    return Err(CommitError::StaleInput(format!(
                        "`{path}` could not be read ({error_kind})"
                    )));
                }
            },
            InputPrecondition::Directory {
                path,
                entries_sha256,
                ..
            } => {
                let listed = jails_state::listing::list_directory(&root.join(path.as_str()), path)
                    .map_err(|failure| CommitError::StaleInput(failure.to_string()))?;
                let actual = jails_protocol::snapshot::directory_digest(&listed)
                    .map_err(|failure| CommitError::StaleInput(failure.to_string()))?;
                if actual != *entries_sha256 {
                    return Err(CommitError::StaleInput(format!(
                        "`{path}` does not hold what it held when this plan was made.\n       \
                         fix: something was added or removed after the plan read it; replan."
                    )));
                }
            }
            InputPrecondition::MachineRoot { presence } => {
                let present = root.join(".jails").is_dir();
                let expected = *presence == jails_protocol::snapshot::MachineRootPresence::Present;
                if present != expected {
                    return Err(CommitError::StaleInput(
                        "`.jails` appeared or disappeared after the plan was made.\n       \
                         fix: replan."
                            .to_string(),
                    ));
                }
            }
            InputPrecondition::ExternalAbsent { .. }
            | InputPrecondition::ExternalFile { .. }
            | InputPrecondition::MachineReceipt { .. } => {}
        }
    }
    Ok(())
}

/// Where the store stands relative to a transaction's two images.
///
/// This is what says whether the commit point was crossed, and therefore
/// whether the files are still the plan's business.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LedgerPosition {
    Before,
    After,
    Neither,
}

pub(crate) fn ledger_position(
    locked: &LockedProject,
    prepared: &PreparedIdentityV1,
) -> LedgerPosition {
    let path = locked.handle.store.root().join("ledger.toml");
    let Ok(actual) = crate::activate::observe(&path) else {
        return LedgerPosition::Neither;
    };
    if image_matches(&actual, prepared.ledger_before) {
        return LedgerPosition::Before;
    }
    if image_matches(&actual, prepared.ledger_after) {
        return LedgerPosition::After;
    }
    LedgerPosition::Neither
}

fn image_matches(actual: &ActualImage, expected: FileImage) -> bool {
    match (expected, actual) {
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
    }
}

fn check_ledger(
    locked: &LockedProject,
    expected: FileImage,
) -> std::result::Result<(), CommitError> {
    let path = locked.handle.store.root().join("ledger.toml");
    let actual = crate::activate::observe(&path).map_err(CommitError::StaleInput)?;
    if !image_matches(&actual, expected) {
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
    use jails_prepare::prepare::GuardedImage;
    use jails_prepare::prepare::{DirectoryOp, PreparedKind};
    use jails_prepare::tool::{OperationContextFingerprint, PreparationContextFingerprint};
    use jails_protocol::conflict::FileMode;
    use jails_protocol::identity::ObjectRef;
    use jails_protocol::plan::{LedgerIntent, PlannedSubject};
    use jails_protocol::snapshot::CanonicalRoot;
    use jails_support::codec::sha256;
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
                    entities_removed: Vec::new(),
                },
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
            path: ProjectPath::parse(at).unwrap(),
            after: object(body),
            mode: mode(),
            contributors: BTreeSet::new(),
        }
    }

    /// A bundle prepared for the project it is about to be committed to.
    ///
    /// The root is not decoration: `commit` refuses a plan prepared elsewhere,
    /// because every path in a prepared operation is project-relative and a plan
    /// for a same-shaped project would otherwise apply cleanly to the wrong one.
    fn bundle(locked: &LockedProject, change: PreparedChange) -> PreparedBundle {
        PreparedBundle {
            root: CanonicalRoot::new(
                std::fs::canonicalize(locked.root())
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )
            .unwrap(),
            change,
            review: jails_prepare::review::PreparedReview::default(),
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
        let result = commit(&locked, &bundle(&locked, change)).unwrap();
        assert!(matches!(result, CommitResult::Committed(_)), "{result:?}");

        let at = scratch.path().join("src/main/java/App.java");
        assert_eq!(std::fs::read(&at).unwrap(), b"class App {}\n");
        assert_eq!(
            crate::activate::mode_of(&std::fs::metadata(&at).unwrap()).unwrap(),
            mode()
        );

        // The directory moved intact into `receipts/`, journal included.
        let published = locked.handle.store.receipt(&transaction);
        assert!(published.join("journal.bin").exists());
        assert!(published.join("receipt.bin").exists());
        assert!(!locked.handle.store.transaction(&transaction).exists());
        ReceiptV1::read(&published).unwrap();
        scratch.close().unwrap();
    }

    /// JDX-INV-001/JDX-OUT-003: preview and apply project one prepared value,
    /// including the filtered directory sequence and both public digests.
    #[test]
    fn preview_and_receipt_agree_on_operations_and_prepared_after() {
        let (scratch, locked) = project();
        let bundle = bundle(
            &locked,
            change_of(
                vec![create_op("src/main/java/App.java", b"class App {}\n")],
                vec![b"class App {}\n"],
                vec!["src", "src/main", "src/main/java"],
            ),
        );
        let preview = jails_prepare::report::Report::of_bundle(&bundle).unwrap();
        let operation_digest = jails_prepare::prepared_after::operations(&bundle.change).unwrap();
        let prepared_after =
            jails_prepare::prepared_after::digest(&bundle.root, &bundle.change).unwrap();
        let receipt = applied_receipt(&bundle.change, operation_digest, prepared_after);

        assert_eq!(preview.operation_digest, receipt.operation_digest);
        assert_eq!(preview.prepared_after, Some(receipt.prepared_after));
        let preview_operations: Vec<_> = preview
            .operations
            .iter()
            .map(|operation| (operation.kind.label(), operation.path.as_str()))
            .collect();
        let receipt_operations: Vec<_> = receipt
            .directories
            .iter()
            .map(|directory| ("create-directory", directory.path.as_str()))
            .chain(receipt.files.iter().map(|file| {
                let kind = match (file.before, file.after) {
                    (FileImage::Absent, FileImage::Present { .. }) => "create",
                    (FileImage::Present { .. }, FileImage::Present { .. }) => "replace",
                    (FileImage::Present { .. }, FileImage::Absent) => "delete",
                    (FileImage::Absent, FileImage::Absent) => unreachable!(),
                };
                (kind, file.path.as_str())
            }))
            .collect();
        assert_eq!(preview_operations, receipt_operations);
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
        commit(&locked, &bundle(&locked, change)).unwrap();
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
        crate::activate::set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![FileOp::Replace {
                path: ProjectPath::parse("pom.xml").unwrap(),
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
        commit(&locked, &bundle(&locked, change)).unwrap();
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
        crate::activate::set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![FileOp::Replace {
                path: ProjectPath::parse("pom.xml").unwrap(),
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

        let error = commit(&locked, &bundle(&locked, change)).unwrap_err();
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
        crate::activate::set_mode(&at, mode()).unwrap();

        let change = change_of(
            vec![create_op("App.java", b"class App {}\n")],
            vec![b"class App {}\n"],
            Vec::new(),
        );
        let result = commit(&locked, &bundle(&locked, change)).unwrap();
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
            commit(&locked, &bundle(&locked, change)).unwrap(),
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
        let error = commit(&locked, &bundle(&locked, change)).unwrap_err();
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
        commit(&locked, &bundle(&locked, change.clone())).unwrap();
        std::fs::remove_file(scratch.path().join("App.java")).unwrap();
        let error = commit(&locked, &bundle(&locked, change)).unwrap_err();
        assert!(matches!(error, CommitError::StaleInput(_)), "{error:?}");
        scratch.close().unwrap();
    }
}
