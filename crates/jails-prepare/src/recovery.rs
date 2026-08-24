//! What recovery did, as a report.
//!
//! plan.md §R3.4 puts these in `CommandEnvelope.recovery`, so they live here
//! rather than beside the executor that produces them: the envelope is the one
//! value a mutation command returns, and it is below the commit layer. The
//! executor re-exports them, so nothing else had to learn a new spelling.
//!
//! An outcome is *observationally clean* when it changed nothing, and those
//! are omitted from the envelope entirely -- the ordinary value is `[]`. What
//! is reported is work an interrupted earlier run left behind and this one
//! finished, which is the only reason a caller would need to know recovery ran
//! at all.

use jails_protocol::effect::{EffectId, EffectState};
use jails_protocol::identity::{OperationId, TransactionId};

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

impl RecoveryTransactionAction {
    /// The kebab-case spelling both renderings use, per §R3.4's encoding rule.
    pub fn label(self) -> &'static str {
        match self {
            Self::AbandonedPrepared => "abandoned-prepared",
            Self::RolledForwardAndPublished => "rolled-forward-and-published",
            Self::PublishedCommittedReceipt => "published-committed-receipt",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverableEffect {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub generation: u64,
    pub effect: EffectId,
    pub state: EffectState,
}
