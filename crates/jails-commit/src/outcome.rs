//! What a commit returns, and the one distinction that matters.
//!
//! ## Why success and failure are not the top-level split
//!
//! plan.md §R4.3: *"Once the ledger commit point is crossed, `commit` never
//! returns `CommitError`."* Everything before that point can refuse and leave
//! the project exactly as it was. Everything after it has already happened,
//! and reporting it as an error would tell the caller to retry work that is
//! already durable.
//!
//! So `CommitError` means *nothing was committed*, and every post-commit
//! problem is a success-side value: a `DeferredError` on the effect, or
//! `CommittedRecoveryRequired` for structural work that failed after the
//! ledger. Both carry what is known and neither fabricates a receipt.
//!
//! ## Why `RecoveredPriorTransaction` is not an error either
//!
//! It means the caller planned against state that recovery has since changed.
//! The plan is stale, but nothing is wrong — the outer driver reloads and
//! replans once. Returning an error would make an ordinary interrupted-run
//! cleanup look like a failure.

use crate::journal::BlockReason;
use jails_prepare::receipt::AppliedReceipt;
use jails_protocol::effect::{EffectId, EffectState};
use jails_protocol::identity::{OperationId, TransactionId};

/// Why a commit did not happen. Nothing was written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitError {
    /// The project moved under the plan.
    StaleInput(String),
    /// Another jails mutation holds the project lock.
    MutationBusy(String),
    /// An external effect is running; no commit may cross its activation.
    EffectBusy(String),
    /// Recovery found something a person has to resolve.
    RecoveryBlocked(BlockReason),
    CorruptMachineState(String),
    /// The prepared value did not validate. A caller bug, not a project one.
    InvalidPrepared(String),
    /// I/O before anything was activated.
    PreActivationIo(String),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInput(why)
            | Self::MutationBusy(why)
            | Self::EffectBusy(why)
            | Self::CorruptMachineState(why)
            | Self::InvalidPrepared(why)
            | Self::PreActivationIo(why) => f.write_str(why),
            Self::RecoveryBlocked(reason) => f.write_str(&reason.explain()),
        }
    }
}

/// Where structural work failed after the ledger commit point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostCommitStage {
    JournalCompletion,
    ReceiptPublication,
    ReceiptReconciliation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostCommitRecoveryError {
    Io(String),
    RecoveryBlocked,
    CorruptMachineState,
}

/// Structural work that failed *after* the commit was durable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedRecoveryRequired {
    pub operation: OperationId,
    pub transaction: TransactionId,
    /// `Some` only when the exact checksum-valid pair was reread. Never
    /// fabricated from the prepared value alone.
    pub receipt: Option<AppliedReceipt>,
    pub stage: PostCommitStage,
    pub error: PostCommitRecoveryError,
}

/// What happened to the one aggregate effect, if there was one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitEffectOutcome {
    NotApplicable,
    Succeeded {
        effect: EffectId,
    },
    Failed {
        effect: EffectId,
    },
    Superseded {
        effect: EffectId,
    },
    /// No trustworthy terminal state was recorded. The receipt projection is
    /// the last checksum-validated one, which may still be pre-terminal.
    DeferredError {
        effect: EffectId,
        error: CommittedEffectError,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommittedEffectError {
    StaleInput,
    CorruptMachineState,
    ReceiptIo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedResult {
    pub receipt: AppliedReceipt,
    pub effect: CommitEffectOutcome,
}

/// What one recovery call actually changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryOutcome {
    /// Every authoritative change this call made, in canonical order. Empty
    /// means recovery was observationally clean — which is the ordinary case
    /// and the one that lets a commit continue.
    pub changes: Vec<RecoveryChange>,
    /// Executable effects recovery reported and did not run.
    pub pending_effects: Vec<RecoverableEffect>,
}

impl RecoveryOutcome {
    pub fn clean() -> Self {
        Self {
            changes: Vec::new(),
            pending_effects: Vec::new(),
        }
    }

    /// Whether the caller's plan is stale because of what recovery did.
    ///
    /// An empty `changes` with a nonempty `pending_effects` is still clean:
    /// reporting an effect nobody ran changed nothing.
    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryChange {
    Transaction {
        operation: OperationId,
        transaction: TransactionId,
        generation: u64,
        action: RecoveryTransactionAction,
    },
    EffectStateChanged {
        operation: OperationId,
        transaction: TransactionId,
        generation: u64,
        effect: EffectId,
        before: EffectState,
        after: EffectState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryTransactionAction {
    AbandonedPrepared,
    RolledForwardAndPublished,
    PublishedCommittedReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverableEffect {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub generation: u64,
    pub effect: EffectId,
    pub state: EffectState,
}

/// What a commit did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitResult {
    /// Nothing to do, for the state rechecked under the lock. Truthful
    /// *because* it is decided after recovery and the precondition recheck
    /// rather than before them.
    NoOp,
    Committed(Box<CommittedResult>),
    CommittedRecoveryRequired(Box<CommittedRecoveryRequired>),
    /// The caller planned against state recovery has since changed. Not an
    /// error: reload and replan once.
    RecoveredPriorTransaction(Box<RecoveryOutcome>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryError {
    MutationBusy(String),
    RecoveryBlocked(BlockReason),
    CorruptMachineState(String),
    Io(String),
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MutationBusy(why) | Self::CorruptMachineState(why) | Self::Io(why) => {
                f.write_str(why)
            }
            Self::RecoveryBlocked(reason) => f.write_str(&reason.explain()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An effect nobody ran changed nothing, so reporting one does not make
    /// the caller's plan stale.
    #[test]
    fn a_reported_but_unrun_effect_leaves_recovery_clean() {
        let outcome = RecoveryOutcome {
            changes: Vec::new(),
            pending_effects: vec![RecoverableEffect {
                operation: OperationId::from_bytes([1; 32]),
                transaction: TransactionId::from_bytes([2; 32]),
                generation: 3,
                effect: EffectId::from_object(jails_protocol::identity::ObjectId::from_bytes(
                    [3; 32],
                )),
                state: EffectState::Deferred,
            }],
        };
        assert!(outcome.is_clean());
    }

    #[test]
    fn a_change_makes_the_callers_plan_stale() {
        let outcome = RecoveryOutcome {
            changes: vec![RecoveryChange::Transaction {
                operation: OperationId::from_bytes([1; 32]),
                transaction: TransactionId::from_bytes([2; 32]),
                generation: 3,
                action: RecoveryTransactionAction::AbandonedPrepared,
            }],
            pending_effects: Vec::new(),
        };
        assert!(!outcome.is_clean());
    }

    /// Every refusal has to say something a person can act on; a code with no
    /// sentence is a dead end.
    #[test]
    fn every_commit_error_renders_a_sentence() {
        for error in [
            CommitError::StaleInput("the project moved".to_string()),
            CommitError::MutationBusy("another run holds the lock".to_string()),
            CommitError::EffectBusy("an effect is running".to_string()),
            CommitError::RecoveryBlocked(BlockReason::RootChanged),
            CommitError::CorruptMachineState("the journal is corrupt".to_string()),
            CommitError::InvalidPrepared("the plan does not validate".to_string()),
            CommitError::PreActivationIo("could not create the directory".to_string()),
        ] {
            assert!(!error.to_string().trim().is_empty(), "{error:?}");
        }
    }
}
