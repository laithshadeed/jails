//! What a mutation command returns, once, in one shape.
//!
//! ## Why an envelope rather than a status code and some prose
//!
//! plan.md §R3.4 makes every mutation command emit exactly one
//! `CommandEnvelope`. The reason is what happens without it: a caller that
//! has to parse prose cannot tell a refusal from a conflict from a no-op, so
//! it keys on the exit code — and the exit code is one byte for a dozen
//! outcomes. The envelope gives the status, the disposition of the commit,
//! the plan, the receipt and the error their own fields, and the exit code
//! becomes a projection rather than the channel.
//!
//! ## Why `ErrorCode` is a closed registry
//!
//! Because it is the part people script against. `message` is free for a
//! command adapter to improve; the code is not, and adding one is an explicit
//! schema change. A command that invented `"stale_input"` beside the
//! registry's `stale-input` would break every consumer that matched on the
//! spelling it had.

use crate::prepare::OperationTarget;
use crate::receipt::{AppliedReceipt, ApplyOutcome};
use crate::recovery::RecoveryOutcome;
use crate::report::Report;
use jails_protocol::effect::{EffectId, EffectState, PostCommitEffect};
use jails_protocol::identity::{OperationId, TransactionId};
use jails_protocol::transition::EffectResumeReason;

/// The schema string a machine reader keys on.
pub const SCHEMA: &str = "jails.command-result.v1";

/// How a command ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandStatus {
    Preview,
    NoOp,
    Applied,
    Conflicted,
    Finalised,
    Aborted,
    EffectRetried,
    EffectSuperseded,
    Refused,
    Stale,
    RecoveryBlocked,
    EffectFailed,
}

impl CommandStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::NoOp => "no-op",
            Self::Applied => "applied",
            Self::Conflicted => "conflicted",
            Self::Finalised => "finalised",
            Self::Aborted => "aborted",
            Self::EffectRetried => "effect-retried",
            Self::EffectSuperseded => "effect-superseded",
            Self::Refused => "refused",
            Self::Stale => "stale",
            Self::RecoveryBlocked => "recovery-blocked",
            Self::EffectFailed => "effect-failed",
        }
    }

    /// §R3.4 fixes these: `0` for a clean preview, no-op, committed success or
    /// a successful or superseded effect retry; `1` for a refusal, stale
    /// input, blocked recovery or effect failure; `2` for a conflict.
    ///
    /// Derived from the status rather than chosen at each call site, because a
    /// command that picked its own would be the one channel a caller cannot
    /// check against the envelope.
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Preview
            | Self::NoOp
            | Self::Applied
            | Self::Finalised
            | Self::Aborted
            | Self::EffectRetried
            | Self::EffectSuperseded => 0,
            Self::Refused | Self::Stale | Self::RecoveryBlocked | Self::EffectFailed => 1,
            Self::Conflicted => 2,
        }
    }
}

/// Whether this invocation crossed a ledger commit point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectCommitDisposition {
    None,
    Existing,
    New,
}

impl ProjectCommitDisposition {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Existing => "existing",
            Self::New => "new",
        }
    }
}

/// The exhaustive v1 error registry.
///
/// A command adapter may add detail to `message`; it may not invent a code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    InvalidRequest,
    InputUnreadable,
    InputInvalid,
    UnsupportedProject,
    LegacyAmbiguous,
    PlanRefused,
    PrepareRefused,
    ToolFailed,
    StaleInput,
    MutationBusy,
    EffectBusy,
    RecoveryBlocked,
    CorruptMachineState,
    EffectFailed,
    InternalInvariant,
}

impl ErrorCode {
    /// Every code, so a test can assert the registry is complete.
    pub const ALL: [ErrorCode; 15] = [
        Self::InvalidRequest,
        Self::InputUnreadable,
        Self::InputInvalid,
        Self::UnsupportedProject,
        Self::LegacyAmbiguous,
        Self::PlanRefused,
        Self::PrepareRefused,
        Self::ToolFailed,
        Self::StaleInput,
        Self::MutationBusy,
        Self::EffectBusy,
        Self::RecoveryBlocked,
        Self::CorruptMachineState,
        Self::EffectFailed,
        Self::InternalInvariant,
    ];

    /// The declaration name in lowercase kebab case.
    pub fn label(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid-request",
            Self::InputUnreadable => "input-unreadable",
            Self::InputInvalid => "input-invalid",
            Self::UnsupportedProject => "unsupported-project",
            Self::LegacyAmbiguous => "legacy-ambiguous",
            Self::PlanRefused => "plan-refused",
            Self::PrepareRefused => "prepare-refused",
            Self::ToolFailed => "tool-failed",
            Self::StaleInput => "stale-input",
            Self::MutationBusy => "mutation-busy",
            Self::EffectBusy => "effect-busy",
            Self::RecoveryBlocked => "recovery-blocked",
            Self::CorruptMachineState => "corrupt-machine-state",
            Self::EffectFailed => "effect-failed",
            Self::InternalInvariant => "internal-invariant",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorReport {
    pub code: ErrorCode,
    pub message: String,
    pub paths: Vec<OperationTarget>,
}

impl ErrorReport {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            paths: Vec::new(),
        }
    }

    pub fn about(mut self, mut paths: Vec<OperationTarget>) -> Self {
        paths.sort();
        self.paths = paths;
        self
    }
}

/// Retrying one already-committed effect.
///
/// It describes no file or ledger operation and reuses the committed
/// operation and transaction ids — which is what lets `--pretend` describe
/// the exact action without preparing a fake project transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectRetryReport {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub effect_index: u32,
    pub effect_id: EffectId,
    pub effect: PostCommitEffect,
    pub reason: EffectResumeReason,
    pub before: EffectState,
    /// `Some` only for a checksum-validated terminal result. A preview, a
    /// recovered prior transaction and every run error leave it `None`,
    /// because the plan did not own a terminal transition.
    pub after: Option<EffectState>,
}

impl EffectRetryReport {
    /// The preview projection: what would be retried, with no outcome.
    pub fn describe(plan: &jails_protocol::transition::EffectRetryPlan) -> Self {
        Self {
            operation: plan.operation,
            transaction: plan.receipt.transaction,
            effect_index: plan.effect_index,
            effect_id: plan.effect_id,
            effect: plan.effect.clone(),
            reason: plan.reason,
            before: plan.expected_state.clone(),
            after: None,
        }
    }

    /// The same, with a terminal state a validated receipt actually recorded.
    pub fn describe_result(
        plan: &jails_protocol::transition::EffectRetryPlan,
        terminal: EffectState,
    ) -> Self {
        let mut report = Self::describe(plan);
        report.after = matches!(
            terminal,
            EffectState::Succeeded | EffectState::Failed { .. } | EffectState::Superseded { .. }
        )
        .then_some(terminal);
        report
    }
}

/// Which of the two report shapes this command produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandReport {
    Prepared(Box<Report>),
    EffectRetry(Box<EffectRetryReport>),
}

/// The one value a mutation command returns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvelope {
    pub status: CommandStatus,
    pub project_commit: ProjectCommitDisposition,
    /// What recovery changed on the way here, in invocation order.
    ///
    /// Ordinarily empty, and empty is the *interesting* case: §R3.4 omits an
    /// observationally clean recovery entirely, so a nonempty list means an
    /// earlier interrupted run left work that this invocation finished. A
    /// caller that could not see that would have no way to tell an ordinary
    /// command from one that also rolled a stranger's transaction forward.
    pub recovery: Vec<RecoveryOutcome>,
    pub report: Option<CommandReport>,
    pub receipt: Option<AppliedReceipt>,
    pub error: Option<ErrorReport>,
}

impl CommandEnvelope {
    /// A preview: a plan, nothing committed.
    pub fn preview(report: Report) -> Self {
        let status = if report.operations.is_empty()
            && report.post_commit.is_empty()
            && report.ledger.kind == crate::report::ReportedLedgerKind::Unchanged
        {
            CommandStatus::NoOp
        } else {
            CommandStatus::Preview
        };
        Self {
            status,
            project_commit: ProjectCommitDisposition::None,
            recovery: Vec::new(),
            report: Some(CommandReport::Prepared(Box::new(report))),
            receipt: None,
            error: None,
        }
    }

    /// A commit that happened. The status is the receipt's own outcome, so a
    /// conflict, a finalisation and an abort cannot be reported as an
    /// ordinary apply by a caller that forgot which it asked for.
    pub fn applied(receipt: AppliedReceipt, project_commit: ProjectCommitDisposition) -> Self {
        let status = match receipt.outcome {
            ApplyOutcome::Applied => CommandStatus::Applied,
            ApplyOutcome::Conflicted => CommandStatus::Conflicted,
            ApplyOutcome::Finalised => CommandStatus::Finalised,
            ApplyOutcome::Aborted => CommandStatus::Aborted,
        };
        Self {
            status,
            project_commit,
            recovery: Vec::new(),
            report: None,
            receipt: Some(receipt),
            error: None,
        }
    }

    /// Nothing to do, decided under the lock.
    ///
    /// §R4.2: a no-op has no receipt. Projecting an empty one would make
    /// "nothing happened" indistinguishable from "everything happened and
    /// changed nothing", and only the second has files to name.
    pub fn no_op() -> Self {
        Self {
            status: CommandStatus::NoOp,
            project_commit: ProjectCommitDisposition::None,
            recovery: Vec::new(),
            report: None,
            receipt: None,
            error: None,
        }
    }

    /// The same envelope, carrying what recovery changed on the way.
    pub fn after_recovery(mut self, recovery: Vec<RecoveryOutcome>) -> Self {
        self.recovery = recovery;
        self
    }

    /// A refusal. Nothing was committed, so there is no receipt to carry.
    pub fn refused(error: ErrorReport) -> Self {
        let status = match error.code {
            ErrorCode::StaleInput => CommandStatus::Stale,
            ErrorCode::RecoveryBlocked => CommandStatus::RecoveryBlocked,
            ErrorCode::EffectFailed => CommandStatus::EffectFailed,
            _ => CommandStatus::Refused,
        };
        Self {
            status,
            project_commit: ProjectCommitDisposition::None,
            recovery: Vec::new(),
            report: None,
            receipt: None,
            error: Some(error),
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.status.exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::tests::{change_with, create};
    use std::collections::BTreeSet;

    #[test]
    fn a_preview_with_work_is_a_preview_and_one_without_is_a_no_op() {
        let with = Report::of(&change_with(vec![create("pom.xml", b"<project/>")])).unwrap();
        assert_eq!(
            CommandEnvelope::preview(with).status,
            CommandStatus::Preview
        );
        let without = Report::of(&change_with(Vec::new())).unwrap();
        assert_eq!(
            CommandEnvelope::preview(without).status,
            CommandStatus::NoOp
        );
    }

    /// The exit code is a projection of the status, not a second channel a
    /// caller has to reconcile with it.
    #[test]
    fn exit_codes_follow_the_status() {
        assert_eq!(CommandStatus::Applied.exit_code(), 0);
        assert_eq!(CommandStatus::NoOp.exit_code(), 0);
        assert_eq!(CommandStatus::EffectSuperseded.exit_code(), 0);
        assert_eq!(CommandStatus::Refused.exit_code(), 1);
        assert_eq!(CommandStatus::EffectFailed.exit_code(), 1);
        assert_eq!(CommandStatus::Conflicted.exit_code(), 2);
    }

    /// The code is what people script against; adding one is a schema change.
    #[test]
    fn every_error_code_has_a_distinct_kebab_case_spelling() {
        let spellings: BTreeSet<&str> = ErrorCode::ALL.iter().map(|code| code.label()).collect();
        assert_eq!(spellings.len(), ErrorCode::ALL.len());
        for spelling in spellings {
            assert!(
                spelling.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "`{spelling}` is not lowercase kebab case"
            );
        }
    }

    #[test]
    fn a_stale_refusal_reports_stale_rather_than_refused() {
        let envelope = CommandEnvelope::refused(ErrorReport::new(
            ErrorCode::StaleInput,
            "the project moved while this ran",
        ));
        assert_eq!(envelope.status, CommandStatus::Stale);
        assert_eq!(envelope.exit_code(), 1);
        assert!(envelope.receipt.is_none());
    }
}
