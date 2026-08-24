//! What a prepared transaction looks like to a person.
//!
//! plan.md §R3.4 makes reporting a projection of the prepared value rather
//! than a second description of the work. That is the whole point: today
//! `--pretend` and the real run interpret the same buckets independently, so
//! a dry run can disagree with what happens. Here there is one value, and the
//! report is a pure function of it.
//!
//! Sorted by path, because a report whose line order depends on a hash map is
//! a report two identical runs disagree about.

use crate::Result;
use crate::prepare::{
    DirectoryOp, FileOp, GuardedImage, OperationTarget, PreparedChange, PreparedKind,
};
use crate::receipt::AppliedReceipt;
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::effect::{EffectState, PostCommitEffect};
use jails_protocol::identity::{ObjectId, OperationId, TransactionId};
use jails_protocol::resource::ResourceOwner;
use std::collections::BTreeSet;

/// The schema string a machine reader keys on.
pub const SCHEMA: &str = "jails.prepared-change.v1";

/// What a reported operation does.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReportedOpKind {
    Create,
    Replace,
    Delete,
    CreateDirectory,
}

impl ReportedOpKind {
    /// The kebab-case spelling both the JSON and the human output use.
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::Delete => "delete",
            Self::CreateDirectory => "create-directory",
        }
    }

    /// The verb the human report prints, which is deliberately shorter.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::Delete => "delete",
            Self::CreateDirectory => "mkdir",
        }
    }
}

/// One operation, as a reader sees it.
///
/// `before`/`after` are content addresses, never bytes: a report that printed
/// file contents would put generated source — and anything a template
/// interpolated into it — into a terminal, a CI log and a JSON payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportedOp {
    pub kind: ReportedOpKind,
    pub path: OperationTarget,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
    pub bytes: Option<u64>,
    /// §R3.4: the exact after-mode for a create or replace, the exact
    /// before-mode for a delete, and `None` only for a directory.
    pub mode: Option<FileMode>,
    pub contributors: BTreeSet<ResourceOwner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportedLedgerKind {
    Unchanged,
    Create,
    Replace,
}

impl ReportedLedgerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Create => "create",
            Self::Replace => "replace",
        }
    }
}

/// What happens to the store.
///
/// The images preserve `Absent`, so a project that has no ledger yet is not
/// rendered as an invented hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportedLedger {
    pub kind: ReportedLedgerKind,
    pub before: FileImage,
    pub after: FileImage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportedEffect {
    pub effect: PostCommitEffect,
    pub state: EffectState,
}

/// Something the reader should know that is not a failure.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WarningCode {
    LegacyUntrusted,
    UnmanagedRetained,
    PostCommitDeferred,
    EnvironmentConstrained,
}

impl WarningCode {
    pub fn label(self) -> &'static str {
        match self {
            Self::LegacyUntrusted => "legacy-untrusted",
            Self::UnmanagedRetained => "unmanaged-retained",
            Self::PostCommitDeferred => "post-commit-deferred",
            Self::EnvironmentConstrained => "environment-constrained",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Warning {
    pub code: WarningCode,
    pub paths: Vec<OperationTarget>,
    pub message: String,
}

/// One presentation-neutral description of a prepared change.
///
/// A *pure projection* over `PreparedChange`, and deliberately not stored
/// inside it. That is the whole fix for the current split: `--pretend` and
/// the real run interpret the same buckets independently today, so a dry run
/// can describe work that differs from what happens. One value, one
/// projection, and the human and JSON renderings are two views of this.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub kind: PreparedKind,
    pub operations: Vec<ReportedOp>,
    pub ledger: ReportedLedger,
    pub post_commit: Vec<ReportedEffect>,
    pub warnings: Vec<Warning>,
}

impl Report {
    /// Project a prepared change. Never reads disk and never replans.
    pub fn of(change: &PreparedChange) -> Result<Self> {
        let mut operations: Vec<ReportedOp> = change.directories.iter().map(directory_op).collect();
        operations.extend(change.operations.iter().map(file_op));

        Ok(Self {
            operation: change.operation_id,
            transaction: change.transaction_id,
            kind: change.kind.clone(),
            operations,
            ledger: ledger_of(change.ledger_before, change.ledger_after),
            post_commit: change
                .post_commit
                .iter()
                .map(|effect| ReportedEffect {
                    effect: effect.clone(),
                    state: EffectState::Deferred,
                })
                .collect(),
            warnings: Vec::new(),
        })
    }

    /// Add a warning, keeping the canonical order §R3.4 specifies.
    pub fn warn(&mut self, mut warning: Warning) {
        warning.paths.sort();
        self.warnings.push(warning);
        self.warnings.sort();
    }
}

fn directory_op(directory: &DirectoryOp) -> ReportedOp {
    ReportedOp {
        kind: ReportedOpKind::CreateDirectory,
        path: OperationTarget::Project(directory.path().clone()),
        before: None,
        after: None,
        bytes: None,
        mode: None,
        contributors: BTreeSet::new(),
    }
}

fn file_op(operation: &FileOp) -> ReportedOp {
    match operation {
        FileOp::Create {
            path,
            after,
            mode,
            contributors,
        } => ReportedOp {
            kind: ReportedOpKind::Create,
            path: path.clone(),
            before: None,
            after: Some(after.id),
            bytes: Some(after.len),
            mode: Some(*mode),
            contributors: contributors.clone(),
        },
        FileOp::Replace {
            path,
            before,
            after,
            mode,
            contributors,
        } => ReportedOp {
            kind: ReportedOpKind::Replace,
            path: path.clone(),
            before: Some(before.object.id),
            after: Some(after.id),
            bytes: Some(after.len),
            mode: Some(*mode),
            contributors: contributors.clone(),
        },
        FileOp::Delete {
            path,
            before: GuardedImage { object, mode },
            contributors,
        } => ReportedOp {
            kind: ReportedOpKind::Delete,
            path: path.clone(),
            before: Some(object.id),
            after: None,
            bytes: Some(object.len),
            mode: Some(*mode),
            contributors: contributors.clone(),
        },
    }
}

fn ledger_of(before: FileImage, after: FileImage) -> ReportedLedger {
    let kind = match (before, after) {
        (a, b) if a == b => ReportedLedgerKind::Unchanged,
        (FileImage::Absent, _) => ReportedLedgerKind::Create,
        _ => ReportedLedgerKind::Replace,
    };
    ReportedLedger {
        kind,
        before,
        after,
    }
}

/// The human rendering.
///
/// Starts `plan <transaction> apply|conflict|finalise|abort`, then one line
/// per operation in the prepared change's own order, then the ledger, the
/// effects and the warnings. It never prints file contents, secrets or an
/// absolute user-template path — a report is read in terminals and CI logs
/// that outlive the run.
pub fn render(report: &Report) -> String {
    let mut out = format!("plan {} {}\n", report.transaction, kind_label(&report.kind));
    for operation in &report.operations {
        let subject = match &operation.path {
            OperationTarget::Project(path) => path.to_string(),
            // Never disguised as an ordinary project output: this is machine
            // state being retired, not a file the user asked to remove.
            OperationTarget::LegacyMachine(path) => format!("legacy-machine {path:?}"),
        };
        out.push_str(&format!("  {:<7} {subject}\n", operation.kind.verb()));
    }
    if report.ledger.kind != ReportedLedgerKind::Unchanged {
        out.push_str(&format!("  ledger  {}\n", report.ledger.kind.label()));
    }
    for effect in &report.post_commit {
        out.push_str(&format!("  effect  {}\n", effect_label(&effect.effect)));
    }
    for warning in &report.warnings {
        out.push_str(&format!(
            "  warn    {}: {}\n",
            warning.code.label(),
            warning.message
        ));
    }
    out
}

pub fn kind_label(kind: &PreparedKind) -> &'static str {
    match kind {
        PreparedKind::Apply => "apply",
        PreparedKind::Conflict { .. } => "conflict",
        PreparedKind::Finalise { .. } => "finalise",
        PreparedKind::Abort { .. } => "abort",
    }
}

fn effect_label(effect: &PostCommitEffect) -> String {
    match effect {
        PostCommitEffect::ComposeReconcile {
            desired_services,
            stop_services,
            ..
        } => format!(
            "compose reconcile ({} up, {} stopped)",
            desired_services.len(),
            stop_services.len()
        ),
    }
}

/// The one-line summary, which has to be able to say "nothing".
///
/// A run that found everything already in place is a real outcome, and
/// printing a confident "applied" over it is how a tool teaches people to
/// stop reading its output.
pub fn summary(report: &Report) -> String {
    match &report.kind {
        PreparedKind::Conflict { paths } => format!(
            "{} file{} could not be merged automatically",
            paths.len(),
            plural(paths.len())
        ),
        PreparedKind::Finalise { .. } => "finishing the frozen conflict".to_string(),
        PreparedKind::Abort { .. } => "putting the frozen conflict back".to_string(),
        PreparedKind::Apply if is_no_op(report) => "nothing to do".to_string(),
        PreparedKind::Apply => {
            let files = report
                .operations
                .iter()
                .filter(|op| op.kind != ReportedOpKind::CreateDirectory)
                .count();
            let mut summary = format!("{files} file{}", plural(files));
            if !report.post_commit.is_empty() {
                summary.push_str(", then reconciling compose services");
            }
            summary
        }
    }
}

fn is_no_op(report: &Report) -> bool {
    report.operations.is_empty()
        && report.post_commit.is_empty()
        && report.ledger.kind == ReportedLedgerKind::Unchanged
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// The human rendering of one command result, per §R3.4.
///
/// One function for both sides, because they are one value: a preview prints
/// what a commit would do and a commit prints what it did, in the same words
/// and the same order. Two renderers would be two vocabularies, and a reader
/// comparing a `--pretend` with the run that followed it would be comparing
/// two descriptions rather than one.
///
/// Recovery comes first, which is §R3.4's order and not a stylistic choice:
/// what an interrupted earlier run left behind and this invocation finished is
/// context for the result, not part of it.
pub fn render_envelope(envelope: &crate::command::CommandEnvelope) -> String {
    let mut out = String::new();
    for outcome in &envelope.recovery {
        for change in &outcome.changes {
            out.push_str(&format!("  recovered {}\n", recovery_line(change)));
        }
        for effect in &outcome.pending_effects {
            out.push_str(&format!(
                "  pending   effect {} from {}\n",
                effect.effect, effect.transaction
            ));
        }
    }
    match (&envelope.report, &envelope.receipt, &envelope.error) {
        (Some(crate::command::CommandReport::Prepared(report)), _, _) => {
            out.push_str(&render(report));
        }
        (Some(crate::command::CommandReport::EffectRetry(retry)), _, _) => {
            out.push_str(&format!(
                "retry effect {} for {}\n",
                retry.effect_id, retry.transaction
            ));
        }
        (None, Some(receipt), _) => out.push_str(&render_receipt(receipt)),
        (None, None, Some(error)) => {
            out.push_str(&format!("{}: {}\n", error.code.label(), error.message));
        }
        // A no-op has no receipt on purpose: §R4.2 keeps "nothing happened"
        // and "everything happened and changed nothing" apart, and only the
        // second has files to name.
        (None, None, None) => out.push_str("nothing to do\n"),
    }
    out
}

/// What a commit did, in the same shape as what a plan would have done.
pub fn render_receipt(receipt: &AppliedReceipt) -> String {
    let mut out = format!("{} {}\n", receipt.outcome.label(), receipt.transaction_id);
    for directory in &receipt.directories {
        out.push_str(&format!("  {:<7} {}\n", "mkdir", directory.path));
    }
    for file in &receipt.files {
        let subject = match &file.path {
            OperationTarget::Project(path) => path.to_string(),
            OperationTarget::LegacyMachine(path) => format!("legacy-machine {path:?}"),
        };
        out.push_str(&format!("  {:<7} {subject}\n", verb_of(file)));
    }
    let ledger = ledger_of(receipt.ledger_before, receipt.ledger_after);
    if ledger.kind != ReportedLedgerKind::Unchanged {
        out.push_str(&format!("  ledger  {}\n", ledger.kind.label()));
    }
    for effect in &receipt.post_commit {
        out.push_str(&format!(
            "  effect  {} ({})\n",
            effect_label(&effect.effect),
            state_label(&effect.state)
        ));
    }
    out
}

/// The verb a receipt row implies, from the pair of images it records.
///
/// Derived rather than stored, because a receipt records *what changed* and
/// the two images already say which of the three it was -- storing the verb
/// beside them would be a fourth value that can disagree with the other three.
fn verb_of(file: &crate::receipt::FileReceipt) -> &'static str {
    match (&file.before, &file.after) {
        (FileImage::Absent, _) => ReportedOpKind::Create.verb(),
        (_, FileImage::Absent) => ReportedOpKind::Delete.verb(),
        _ => ReportedOpKind::Replace.verb(),
    }
}

fn state_label(state: &EffectState) -> &'static str {
    match state {
        EffectState::Deferred => "deferred",
        EffectState::Pending { .. } => "pending",
        EffectState::Running { .. } => "running",
        EffectState::Succeeded => "done",
        EffectState::Failed { .. } => "failed",
        EffectState::Superseded { .. } => "superseded",
    }
}

fn recovery_line(change: &crate::recovery::RecoveryChange) -> String {
    use crate::recovery::RecoveryChange;
    match change {
        RecoveryChange::Transaction {
            transaction,
            action,
            ..
        } => format!("{} {transaction}", action.label()),
        RecoveryChange::EffectStateChanged {
            effect,
            before,
            after,
            ..
        } => format!(
            "effect {effect} {} -> {}",
            state_label(before),
            state_label(after)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::tests::{change_with, create};

    /// The projection follows the prepared change's own order, which is
    /// path order: a report whose lines depend on a hash map is a report two
    /// identical runs disagree about.
    #[test]
    fn operations_follow_the_prepared_order() {
        let change = change_with(vec![
            create("src/main/java/com/example/demo/Zebra.java", b"z"),
            create("src/main/java/com/example/demo/Apple.java", b"a"),
        ]);
        let report = Report::of(&change).unwrap();
        let rendered: Vec<String> = report
            .operations
            .iter()
            .map(|op| match &op.path {
                OperationTarget::Project(path) => path.to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            rendered,
            vec![
                "src/main/java/com/example/demo/Apple.java",
                "src/main/java/com/example/demo/Zebra.java"
            ]
        );
    }

    /// Printing a confident "applied" over a run that did nothing is how a
    /// tool teaches people to stop reading its output.
    #[test]
    fn a_change_with_nothing_to_do_says_so() {
        let report = Report::of(&change_with(Vec::new())).unwrap();
        assert_eq!(summary(&report), "nothing to do");
    }

    #[test]
    fn one_file_is_not_reported_as_one_files() {
        let report = Report::of(&change_with(vec![create("pom.xml", b"<project/>")])).unwrap();
        assert_eq!(summary(&report), "1 file");
    }

    /// A report that printed file contents would put generated source — and
    /// anything a template interpolated into it — into a terminal and a CI log.
    #[test]
    fn a_report_carries_content_addresses_and_never_content() {
        let change = change_with(vec![create("pom.xml", b"<project>secret</project>")]);
        let report = Report::of(&change).unwrap();
        let text = render(&report);
        assert!(!text.contains("secret"), "{text}");
        assert_eq!(report.operations[0].bytes, Some(25));
        assert!(report.operations[0].after.is_some());
    }

    /// §R3.4: the exact after-mode for a create or replace, the exact
    /// before-mode for a delete, and `None` only for a directory.
    #[test]
    fn only_a_directory_reports_no_mode() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        change
            .directories
            .push(crate::prepare::DirectoryOp::Create {
                path: jails_protocol::identity::ProjectPath::parse("src").unwrap(),
            });
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        let report = Report::of(&change).unwrap();
        for operation in &report.operations {
            assert_eq!(
                operation.mode.is_none(),
                operation.kind == ReportedOpKind::CreateDirectory,
                "{operation:?}"
            );
        }
    }

    /// A project that has no ledger yet must not be rendered as an invented
    /// hash, so the images preserve `Absent`.
    #[test]
    fn an_absent_ledger_stays_absent_in_the_report() {
        let report = Report::of(&change_with(Vec::new())).unwrap();
        assert_eq!(report.ledger.kind, ReportedLedgerKind::Unchanged);
        assert_eq!(report.ledger.before, FileImage::Absent);
    }

    /// Machine state being retired is not a file the user asked to remove.
    #[test]
    fn a_legacy_target_is_labelled_rather_than_disguised() {
        let mut report = Report::of(&change_with(Vec::new())).unwrap();
        report.operations.push(ReportedOp {
            kind: ReportedOpKind::Delete,
            path: OperationTarget::LegacyMachine(
                jails_protocol::snapshot::LegacySourcePath::VersionFile,
            ),
            before: None,
            after: None,
            bytes: None,
            mode: None,
            contributors: BTreeSet::new(),
        });
        assert!(render(&report).contains("legacy-machine"));
    }

    #[test]
    fn warnings_sort_by_code_then_path_then_message() {
        let mut report = Report::of(&change_with(Vec::new())).unwrap();
        report.warn(Warning {
            code: WarningCode::UnmanagedRetained,
            paths: Vec::new(),
            message: "kept two hand-written properties".to_string(),
        });
        report.warn(Warning {
            code: WarningCode::LegacyUntrusted,
            paths: Vec::new(),
            message: "one row of unknown origin".to_string(),
        });
        assert_eq!(report.warnings[0].code, WarningCode::LegacyUntrusted);
    }
}
