//! Finishing what an interrupted run started — forward, never backward.
//!
//! ## Why forward
//!
//! plan.md §R4 makes the default crash policy roll a validated journal
//! *forward*. Rolling back would need a preimage for every operation and a
//! guarantee that nothing else has touched the tree since — and the second is
//! exactly what an interrupted run cannot promise. Rolling forward needs the
//! prepared bytes, which the journal already carries, and it converges: the
//! same work applied twice lands in the same place.
//!
//! Preimages still exist, but for a *guarded explicit* abort and for audit.
//! An abort is itself an ordinary forward transaction with its own journal,
//! not a reverse mode of this one.
//!
//! ## Why the ledger classifies first
//!
//! The ledger says whether the commit point was crossed. Below it, the files
//! are still the plan's business and every remaining operation may be
//! applied. Above it, the transaction is *true* and the only work left is
//! structural — and applying a file operation there would overwrite whatever
//! the user has done since.
//!
//! ## Why a block is reclassified rather than trusted
//!
//! A `Blocked` journal records what stopped the last run. That is diagnostic,
//! not a permanent veto: a person who restores the named file must be able to
//! continue without editing a journal by hand. So a retry reclassifies
//! everything from scratch and either advances or rewrites the block with
//! whatever fails *now*.

use crate::execute::{
    LedgerFailure, LedgerPosition, LockedProject, apply_operations, ledger_position, write_ledger,
};
use crate::journal::{JournalState, JournalV1, ResumeState};
use crate::outcome::{RecoveryChange, RecoveryError, RecoveryOutcome, RecoveryTransactionAction};
use crate::store;
use jails_protocol::identity::TransactionId;
use std::path::{Path, PathBuf};

/// Finish or classify every incomplete transaction. Idempotent.
///
/// Structural only: it completes journals, publishes receipts and reports
/// what it found. It never starts an external process, and it never
/// reconstructs desire from a receipt — the ledger owns that.
pub fn recover_locked(
    locked: &LockedProject,
) -> std::result::Result<RecoveryOutcome, RecoveryError> {
    let mut outcome = RecoveryOutcome::clean();
    let directories = incomplete(locked)?;

    // More than one is corruption: nothing says which came first, and
    // ordering them by mtime would be exactly the guess §R4.4 forbids.
    if directories.len() > 1 {
        return Err(RecoveryError::RecoveryBlocked(
            crate::journal::BlockReason::MultipleTransactions,
        ));
    }
    let Some(directory) = directories.into_iter().next() else {
        return Ok(outcome);
    };

    let journal = match JournalV1::read(&directory) {
        Ok(journal) => journal,
        Err(error) => {
            // A directory with no journal that never had one is unvalidated
            // staging and may be removed. One whose journal does not decode
            // is preserved: it may be the only record of what was meant.
            if directory.join("journal.bin").exists() {
                return Err(RecoveryError::CorruptMachineState(error));
            }
            std::fs::remove_dir_all(&directory)
                .map_err(|error| RecoveryError::Io(error.to_string()))?;
            return Ok(outcome);
        }
    };

    // A project moved or replaced under a transaction is not the project the
    // plan was made against, and comparing paths would not notice.
    if journal.root_identity != locked.root_identity() {
        return Err(RecoveryError::RecoveryBlocked(
            crate::journal::BlockReason::RootChanged,
        ));
    }

    // `effective_state` resolves a block to the phase it would resume from,
    // so `Blocked` is not one of the arms: a retry reclassifies from scratch
    // and either advances or blocks again on whatever fails now.
    match effective_state(&journal) {
        ResumeState::Prepared => {
            // `Prepared` promises no live mutation, so nothing needs undoing
            // and the directory is removable. Recovery never promotes it to
            // `Active`: the run that stopped had not decided to act.
            std::fs::remove_dir_all(&directory)
                .map_err(|error| RecoveryError::Io(error.to_string()))?;
            outcome.changes.push(RecoveryChange::Transaction {
                operation: journal.prepared.operation_id,
                transaction: journal.transaction,
                generation: journal.generation,
                action: RecoveryTransactionAction::AbandonedPrepared,
            });
        }
        ResumeState::Active | ResumeState::LedgerCommitted | ResumeState::Complete => {
            // The ledger says whether the commit point was crossed. Below it
            // the files are still the plan's business and every remaining
            // operation is applied; above it the transaction is *true* and
            // the only work left is structural — applying a file operation
            // there would overwrite whatever the user has done since.
            match ledger_position(locked, &journal.prepared) {
                LedgerPosition::Before => {
                    roll_forward(locked, &directory, &journal)?;
                }
                LedgerPosition::After => {}
                LedgerPosition::Neither => {
                    return Err(RecoveryError::RecoveryBlocked(
                        crate::journal::BlockReason::UnknownLiveImage {
                            actual: crate::journal::ActualImage::Other,
                        },
                    ));
                }
            }
            finish(locked, &directory, &journal, &mut outcome)?;
        }
    }
    Ok(outcome)
}

/// The phase to dispatch on.
///
/// A [`ResumeState`] rather than a [`JournalState`] on purpose: a block names
/// the phase it would resume from, so once it is resolved there is no
/// `Blocked` case left to handle — and the type says so rather than leaving
/// an arm that can never run.
fn effective_state(journal: &JournalV1) -> ResumeState {
    match &journal.state {
        JournalState::Prepared => ResumeState::Prepared,
        JournalState::Active => ResumeState::Active,
        JournalState::LedgerCommitted => ResumeState::LedgerCommitted,
        JournalState::Complete => ResumeState::Complete,
        JournalState::Blocked { resume, .. } => *resume,
    }
}

/// Apply every operation this transaction has not reached yet, then commit
/// the ledger.
///
/// The same code the first attempt ran, because a recovery pass that used a
/// different one would be a second implementation of the thing that must not
/// disagree with itself. Each operation reclassifies immediately before
/// acting, so an already-applied one is skipped and an unrecognised one
/// stops.
fn roll_forward(
    locked: &LockedProject,
    directory: &Path,
    journal: &JournalV1,
) -> std::result::Result<(), RecoveryError> {
    let objects = directory.join("objects");
    if let Err(blocked) = apply_operations(locked, &journal.prepared, directory, &objects) {
        let reason = blocked.reason.clone();
        // Record the block so the next run reads it rather than
        // rediscovering it, then refuse.
        let _ = blocked.into_error(directory, journal);
        return Err(RecoveryError::RecoveryBlocked(reason));
    }
    match write_ledger(locked, &journal.prepared, directory, &objects) {
        Ok(()) => Ok(()),
        Err(LedgerFailure::BeforeCommit(why) | LedgerFailure::AfterCommit(why)) => {
            Err(RecoveryError::Io(why))
        }
    }
}

/// Complete the journal, publish the receipt, and move the intact directory.
fn finish(
    locked: &LockedProject,
    directory: &Path,
    journal: &JournalV1,
    outcome: &mut RecoveryOutcome,
) -> std::result::Result<(), RecoveryError> {
    let complete = journal.advanced(JournalState::Complete);
    complete.persist(directory).map_err(RecoveryError::Io)?;

    let receipt = crate::journal::ReceiptV1 {
        transaction: complete.transaction,
        generation: complete.generation,
        prepared: complete.prepared.clone(),
        complete_journal_checksum: crate::journal::ReceiptV1::witness_of(&complete)
            .map_err(RecoveryError::CorruptMachineState)?,
        post_commit: Vec::new(),
    };
    receipt.persist(directory).map_err(RecoveryError::Io)?;

    let _ = std::fs::remove_dir_all(directory.join("live-temp"));
    store::sync_dir(directory).map_err(RecoveryError::Io)?;

    let destination = locked.handle().store().receipt(&journal.transaction);
    if destination.exists() {
        // Both placements is corruption: two directories claim one
        // transaction and nothing says which is authoritative.
        return Err(RecoveryError::CorruptMachineState(format!(
            "transaction {} exists both as staging and as a published receipt",
            journal.transaction
        )));
    }
    std::fs::rename(directory, &destination)
        .map_err(|error| RecoveryError::Io(error.to_string()))?;
    for parent in [
        locked.handle().store().transactions(),
        locked.handle().store().receipts(),
    ] {
        store::sync_dir(&parent).map_err(RecoveryError::Io)?;
    }

    outcome.changes.push(RecoveryChange::Transaction {
        operation: journal.prepared.operation_id,
        transaction: journal.transaction,
        generation: journal.generation,
        action: RecoveryTransactionAction::RolledForwardAndPublished,
    });
    Ok(())
}

/// Every transaction directory that is not yet published.
fn incomplete(locked: &LockedProject) -> std::result::Result<Vec<PathBuf>, RecoveryError> {
    let transactions = locked.handle().store().transactions();
    let entries = match std::fs::read_dir(&transactions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(RecoveryError::Io(error.to_string())),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| RecoveryError::Io(error.to_string()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // A directory whose name is not a transaction id is not one of ours.
        // Removing it would be acting on something nothing recorded.
        if TransactionId::parse_hex(&name).is_err() {
            continue;
        }
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::ProjectHandle;
    use crate::journal::{ActualImage, BlockReason, RootIdentity};
    use jails_prepare::operation::{ApplySemantics, OperationIdentityV1, OperationSemanticsV1};
    use jails_prepare::prepare::{FileOp, OperationTarget, PreparedIdentityV1, PreparedKind};
    use jails_prepare::tool::{OperationContextFingerprint, PreparationContextFingerprint};
    use jails_protocol::conflict::{FileImage, FileMode};
    use jails_protocol::identity::{ObjectId, ObjectRef, ProjectPath};
    use jails_protocol::plan::{LedgerIntent, PlannedSubject};
    use jails_support::codec::sha256;
    use jails_support::scratch::ScratchDir;
    use std::collections::BTreeSet;

    fn prepared() -> PreparedIdentityV1 {
        let body = b"class App {}\n";
        let after = ObjectRef::new(ObjectId::from_bytes(sha256(body)), body.len() as u64);
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
        PreparedIdentityV1 {
            operation_id: operation_identity.operation_id().unwrap(),
            operation_identity,
            preparation: PreparationContextFingerprint::default(),
            input_preconditions: Vec::new(),
            operations: vec![FileOp::Create {
                path: OperationTarget::Project(ProjectPath::parse("App.java").unwrap()),
                after,
                mode: FileMode::new(0o644).unwrap(),
                contributors: BTreeSet::new(),
            }],
            directories: Vec::new(),
            ledger_before: FileImage::Absent,
            ledger_after: FileImage::Absent,
            object_manifest: vec![after],
            post_commit: Vec::new(),
            kind: PreparedKind::Apply,
        }
    }

    fn project() -> (ScratchDir, LockedProject) {
        let scratch = ScratchDir::in_temp("jails-recover").unwrap();
        let handle = ProjectHandle::at(scratch.path()).unwrap();
        let locked = LockedProject::acquire(handle, "test").unwrap();
        (scratch, locked)
    }

    fn stage(locked: &LockedProject, state: JournalState) -> (PathBuf, JournalV1) {
        let prepared = prepared();
        let journal = JournalV1 {
            transaction: prepared.transaction_id().unwrap(),
            generation: 1,
            root_identity: locked.root_identity(),
            state,
            prepared,
        };
        let directory = locked.handle().store().transaction(&journal.transaction);
        store::create_private_dir(&directory).unwrap();
        // The bytes the transaction will write live with it. A journal
        // without them is a transaction that cannot be finished, which is a
        // different test.
        let body = b"class App {}\n";
        store::put_object(
            &directory.join("objects"),
            &ObjectId::from_bytes(sha256(body)),
            body,
        )
        .unwrap();
        journal.persist(&directory).unwrap();
        (directory, journal)
    }

    /// An empty store is the ordinary case, and it must be clean — otherwise
    /// every commit would think its plan was stale.
    #[test]
    fn a_project_with_nothing_to_recover_is_clean() {
        let (scratch, locked) = project();
        assert!(recover_locked(&locked).unwrap().is_clean());
        scratch.close().unwrap();
    }

    /// `Prepared` promises no live mutation, so the directory is removable.
    /// Recovery never promotes it: the run that stopped had not decided to
    /// act.
    #[test]
    fn a_prepared_transaction_is_abandoned_not_promoted() {
        let (scratch, locked) = project();
        let (directory, journal) = stage(&locked, JournalState::Prepared);
        let outcome = recover_locked(&locked).unwrap();
        assert!(!directory.exists());
        assert!(
            !locked
                .handle()
                .store()
                .receipt(&journal.transaction)
                .exists()
        );
        assert_eq!(
            outcome.changes,
            vec![RecoveryChange::Transaction {
                operation: journal.prepared.operation_id,
                transaction: journal.transaction,
                generation: 1,
                action: RecoveryTransactionAction::AbandonedPrepared,
            }]
        );
        scratch.close().unwrap();
    }

    #[test]
    fn an_active_transaction_is_finished_forward_and_published() {
        let (scratch, locked) = project();
        let (directory, journal) = stage(&locked, JournalState::Active);
        let outcome = recover_locked(&locked).unwrap();
        assert!(!directory.exists());

        let published = locked.handle().store().receipt(&journal.transaction);
        crate::journal::ReceiptV1::read(&published).unwrap();
        assert!(matches!(
            outcome.changes[0],
            RecoveryChange::Transaction {
                action: RecoveryTransactionAction::RolledForwardAndPublished,
                ..
            }
        ));
        scratch.close().unwrap();
    }

    /// Recovery converges: running it again finds nothing to do.
    #[test]
    fn recovery_is_idempotent() {
        let (scratch, locked) = project();
        stage(&locked, JournalState::Active);
        recover_locked(&locked).unwrap();
        assert!(recover_locked(&locked).unwrap().is_clean());
        scratch.close().unwrap();
    }

    /// A person who restores the named file must be able to continue without
    /// editing a journal by hand — so a block records what stopped the last
    /// run and is not a permanent veto.
    #[test]
    fn a_blocked_journal_resumes_from_the_phase_it_names() {
        let (scratch, locked) = project();
        let (directory, journal) = stage(
            &locked,
            JournalState::Blocked {
                resume: ResumeState::Prepared,
                path: Some(ProjectPath::parse("App.java").unwrap()),
                reason: BlockReason::UnknownLiveImage {
                    actual: ActualImage::Directory,
                },
            },
        );
        let outcome = recover_locked(&locked).unwrap();
        assert!(!directory.exists(), "the block outlived the condition");
        assert_eq!(
            outcome.changes,
            vec![RecoveryChange::Transaction {
                operation: journal.prepared.operation_id,
                transaction: journal.transaction,
                generation: 1,
                action: RecoveryTransactionAction::AbandonedPrepared,
            }]
        );
        scratch.close().unwrap();
    }

    /// The same block over a transaction that had already activated resumes
    /// forward instead, because that is the phase it names.
    #[test]
    fn a_block_over_an_activated_transaction_resumes_forward() {
        let (scratch, locked) = project();
        let (_, journal) = stage(
            &locked,
            JournalState::Blocked {
                resume: ResumeState::Active,
                path: None,
                reason: BlockReason::Unreadable {
                    error_kind: "permission denied".to_string(),
                },
            },
        );
        recover_locked(&locked).unwrap();
        crate::journal::ReceiptV1::read(&locked.handle().store().receipt(&journal.transaction))
            .unwrap();
        scratch.close().unwrap();
    }

    /// Nothing says which of two came first, and ordering them by mtime would
    /// be exactly the guess §R4.4 forbids.
    #[test]
    fn more_than_one_incomplete_transaction_blocks_every_mutation() {
        let (scratch, locked) = project();
        stage(&locked, JournalState::Active);
        let second = locked.handle().store().transactions().join("a".repeat(64));
        store::create_private_dir(&second).unwrap();
        let error = recover_locked(&locked).unwrap_err();
        assert!(
            matches!(
                error,
                RecoveryError::RecoveryBlocked(BlockReason::MultipleTransactions)
            ),
            "{error:?}"
        );
        scratch.close().unwrap();
    }

    /// A directory that never had a journal is unvalidated staging.
    #[test]
    fn a_directory_with_no_journal_is_removed_as_staging() {
        let (scratch, locked) = project();
        let orphan = locked.handle().store().transactions().join("b".repeat(64));
        store::create_private_dir(&orphan).unwrap();
        assert!(recover_locked(&locked).unwrap().is_clean());
        assert!(!orphan.exists());
        scratch.close().unwrap();
    }

    /// One whose journal does not decode is preserved: it may be the only
    /// record of what was meant.
    #[test]
    fn a_corrupt_journal_is_preserved_and_blocks() {
        let (scratch, locked) = project();
        let (directory, _) = stage(&locked, JournalState::Active);
        std::fs::write(directory.join("journal.bin"), b"not a journal").unwrap();
        let error = recover_locked(&locked).unwrap_err();
        assert!(
            matches!(error, RecoveryError::CorruptMachineState(_)),
            "{error:?}"
        );
        assert!(directory.exists(), "a corrupt journal was removed");
        scratch.close().unwrap();
    }

    /// A project moved or replaced under a transaction is not the project the
    /// plan was made against, and comparing paths would not notice.
    #[test]
    fn a_transaction_from_another_root_blocks() {
        let (scratch, locked) = project();
        let prepared = prepared();
        let journal = JournalV1 {
            transaction: prepared.transaction_id().unwrap(),
            generation: 1,
            root_identity: RootIdentity {
                device: 1,
                inode: 1,
            },
            state: JournalState::Active,
            prepared,
        };
        let directory = locked.handle().store().transaction(&journal.transaction);
        store::create_private_dir(&directory).unwrap();
        journal.persist(&directory).unwrap();

        let error = recover_locked(&locked).unwrap_err();
        assert!(
            matches!(
                error,
                RecoveryError::RecoveryBlocked(BlockReason::RootChanged)
            ),
            "{error:?}"
        );
        scratch.close().unwrap();
    }
}
