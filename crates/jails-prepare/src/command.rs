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

use crate::receipt::{AppliedReceipt, ApplyOutcome};
use crate::recovery::RecoveryOutcome;
use crate::report::Report;
use jails_protocol::effect::{EffectId, EffectState, PostCommitEffect};
use jails_protocol::identity::ProjectPath;
use jails_protocol::identity::{OperationId, TransactionId};
use jails_protocol::request::RequestSyntaxFingerprint;
use jails_protocol::transition::EffectResumeReason;

/// The schema string a machine reader keys on.
pub(crate) const SCHEMA: &str = "jails.command-result.v1";

/// The current command-result schema.
pub const SCHEMA_V2: &str = "jails.command-result.v2";

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
    pub const ALL: [ErrorCode; 14] = [
        Self::InvalidRequest,
        Self::InputUnreadable,
        Self::InputInvalid,
        Self::UnsupportedProject,
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

/// Stable machine-readable diagnostic codes introduced with command-result
/// v2. The v1 [`ErrorCode`] registry is deliberately left unchanged.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCode {
    InvalidRequest,
    InputUnreadable,
    InputInvalid,
    UnsupportedProject,
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
    SqlParse,
    SqlUnverified,
    SchemaDrift,
    MigrationRisk,
    MigrationSealed,
    MigrationEditedAfterSeal,
    StoragePolicyRequired,
    ResourceInconsistent,
    ResourceNotRevivable,
    DataPlanRequired,
    StorageDependencyBlocked,
    ContractBreaking,
    VerificationFailed,
    ServiceUnavailable,
    ProtocolMismatch,
    WatchOverflow,
}

impl DiagnosticCode {
    pub const ALL: [Self; 30] = [
        Self::InvalidRequest,
        Self::InputUnreadable,
        Self::InputInvalid,
        Self::UnsupportedProject,
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
        Self::SqlParse,
        Self::SqlUnverified,
        Self::SchemaDrift,
        Self::MigrationRisk,
        Self::MigrationSealed,
        Self::MigrationEditedAfterSeal,
        Self::StoragePolicyRequired,
        Self::ResourceInconsistent,
        Self::ResourceNotRevivable,
        Self::DataPlanRequired,
        Self::StorageDependencyBlocked,
        Self::ContractBreaking,
        Self::VerificationFailed,
        Self::ServiceUnavailable,
        Self::ProtocolMismatch,
        Self::WatchOverflow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid-request",
            Self::InputUnreadable => "input-unreadable",
            Self::InputInvalid => "input-invalid",
            Self::UnsupportedProject => "unsupported-project",
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
            Self::SqlParse => "sql-parse",
            Self::SqlUnverified => "sql-unverified",
            Self::SchemaDrift => "schema-drift",
            Self::MigrationRisk => "migration-risk",
            Self::MigrationSealed => "migration-sealed",
            Self::MigrationEditedAfterSeal => "migration-edited-after-seal",
            Self::StoragePolicyRequired => "storage-policy-required",
            Self::ResourceInconsistent => "resource-inconsistent",
            Self::ResourceNotRevivable => "resource-not-revivable",
            Self::DataPlanRequired => "data-plan-required",
            Self::StorageDependencyBlocked => "storage-dependency-blocked",
            Self::ContractBreaking => "contract-breaking",
            Self::VerificationFailed => "verification-failed",
            Self::ServiceUnavailable => "service-unavailable",
            Self::ProtocolMismatch => "protocol-mismatch",
            Self::WatchOverflow => "watch-overflow",
        }
    }
}

impl From<ErrorCode> for DiagnosticCode {
    fn from(value: ErrorCode) -> Self {
        match value {
            ErrorCode::InvalidRequest => Self::InvalidRequest,
            ErrorCode::InputUnreadable => Self::InputUnreadable,
            ErrorCode::InputInvalid => Self::InputInvalid,
            ErrorCode::UnsupportedProject => Self::UnsupportedProject,
            ErrorCode::PlanRefused => Self::PlanRefused,
            ErrorCode::PrepareRefused => Self::PrepareRefused,
            ErrorCode::ToolFailed => Self::ToolFailed,
            ErrorCode::StaleInput => Self::StaleInput,
            ErrorCode::MutationBusy => Self::MutationBusy,
            ErrorCode::EffectBusy => Self::EffectBusy,
            ErrorCode::RecoveryBlocked => Self::RecoveryBlocked,
            ErrorCode::CorruptMachineState => Self::CorruptMachineState,
            ErrorCode::EffectFailed => Self::EffectFailed,
            ErrorCode::InternalInvariant => Self::InternalInvariant,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorReport {
    pub code: ErrorCode,
    pub message: String,
    pub paths: Vec<ProjectPath>,
}

impl ErrorReport {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            paths: Vec::new(),
        }
    }

    pub fn about(mut self, mut paths: Vec<ProjectPath>) -> Self {
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

/// The command whose semantic result this envelope contains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandIdentity {
    pub path: Vec<String>,
    pub fingerprint: RequestSyntaxFingerprint,
    pub read_only: bool,
}

/// V2 keeps every v1 status and adds the successful read-only result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandStatusV2 {
    Succeeded,
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

impl CommandStatusV2 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
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

    pub fn exit_code(self) -> u8 {
        match self {
            Self::Succeeded
            | Self::Preview
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

impl From<CommandStatus> for CommandStatusV2 {
    fn from(value: CommandStatus) -> Self {
        match value {
            CommandStatus::Preview => Self::Preview,
            CommandStatus::NoOp => Self::NoOp,
            CommandStatus::Applied => Self::Applied,
            CommandStatus::Conflicted => Self::Conflicted,
            CommandStatus::Finalised => Self::Finalised,
            CommandStatus::Aborted => Self::Aborted,
            CommandStatus::EffectRetried => Self::EffectRetried,
            CommandStatus::EffectSuperseded => Self::EffectSuperseded,
            CommandStatus::Refused => Self::Refused,
            CommandStatus::Stale => Self::Stale,
            CommandStatus::RecoveryBlocked => Self::RecoveryBlocked,
            CommandStatus::EffectFailed => Self::EffectFailed,
        }
    }
}

/// Report kinds supported during the first v2 work package. Later v2-only
/// report packages extend this closed registry before v2 is released.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandReportV2 {
    Prepared(Box<Report>),
    EffectRetry(Box<EffectRetryReport>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorReportV2 {
    pub code: DiagnosticCode,
    pub message: String,
    /// Typed diagnostics are introduced by their owning vertical packages.
    /// Existing v1 errors map without inventing source evidence.
    pub diagnostics: Vec<Diagnostic>,
}

/// A diagnostic value is deliberately uninhabited in DX-001. This reserves
/// the exact array field without fabricating source ranges or typed fixes;
/// the packages that produce such evidence introduce the complete value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {}

/// The current machine-readable command result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvelopeV2 {
    pub command: CommandIdentity,
    pub status: CommandStatusV2,
    pub project_commit: ProjectCommitDisposition,
    pub recovery: Vec<RecoveryOutcome>,
    pub report: Option<CommandReportV2>,
    pub receipt: Option<AppliedReceipt>,
    pub error: Option<ErrorReportV2>,
    pub timings: Vec<crate::timing::TimingSpan>,
}

impl CommandEnvelopeV2 {
    /// Project a frozen v1 mutation result into the current envelope without
    /// changing the v1 model or serializer.
    pub fn from_v1(command: CommandIdentity, value: &CommandEnvelope) -> Self {
        let report = value.report.as_ref().map(|report| match report {
            CommandReport::Prepared(report) => CommandReportV2::Prepared(report.clone()),
            CommandReport::EffectRetry(report) => CommandReportV2::EffectRetry(report.clone()),
        });
        let error = value.error.as_ref().map(|error| ErrorReportV2 {
            code: error.code.into(),
            message: error.message.clone(),
            diagnostics: Vec::new(),
        });
        Self {
            command,
            status: value.status.into(),
            project_commit: value.project_commit,
            recovery: value.recovery.clone(),
            report,
            receipt: value.receipt.clone(),
            error,
            timings: value.timings.clone(),
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.status.exit_code()
    }
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
    /// Runtime observations only; never part of a prepared or committed
    /// transition's identity.
    pub timings: Vec<crate::timing::TimingSpan>,
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
            timings: Vec::new(),
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
            timings: Vec::new(),
        }
    }

    /// The project commit succeeded but its external effect did not. The
    /// receipt remains present because it is the retry authority.
    pub fn effect_failed(mut self, message: impl Into<String>) -> Self {
        self.status = CommandStatus::EffectFailed;
        self.error = Some(ErrorReport::new(ErrorCode::EffectFailed, message));
        self
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
            timings: Vec::new(),
        }
    }

    /// The same envelope, carrying what recovery changed on the way.
    pub fn after_recovery(mut self, recovery: Vec<RecoveryOutcome>) -> Self {
        self.recovery = recovery;
        self
    }

    /// The same result with invocation-local performance observations.
    pub fn with_timings(mut self, timings: Vec<crate::timing::TimingSpan>) -> Self {
        self.timings = timings;
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
            timings: Vec::new(),
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
    fn v2_diagnostic_registry_is_complete_distinct_and_kebab_case() {
        let spellings: BTreeSet<&str> = DiagnosticCode::ALL
            .iter()
            .map(|code| code.label())
            .collect();
        assert_eq!(spellings.len(), DiagnosticCode::ALL.len());
        for spelling in spellings {
            assert!(
                spelling.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "`{spelling}` is not lowercase kebab case"
            );
        }
        assert!(DiagnosticCode::ALL.contains(&DiagnosticCode::ProtocolMismatch));
        assert!(DiagnosticCode::ALL.contains(&DiagnosticCode::WatchOverflow));
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
