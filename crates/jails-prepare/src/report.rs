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
use crate::prepare::{DirectoryOp, FileOp, GuardedImage, PreparedChange, PreparedKind};
use crate::receipt::AppliedReceipt;
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::effect::{EffectState, PostCommitEffect};
use jails_protocol::identity::ProjectPath;
use jails_protocol::identity::{ObjectId, OperationId, TransactionId};
use jails_protocol::resource::ResourceOwner;
use std::collections::BTreeSet;

/// The schema string a machine reader keys on.
pub(crate) const SCHEMA: &str = "jails.prepared-change.v1";

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
    pub path: ProjectPath,
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
    UnmanagedRetained,
    PostCommitDeferred,
    EnvironmentConstrained,
}

impl WarningCode {
    pub fn label(self) -> &'static str {
        match self {
            Self::UnmanagedRetained => "unmanaged-retained",
            Self::PostCommitDeferred => "post-commit-deferred",
            Self::EnvironmentConstrained => "environment-constrained",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Warning {
    pub code: WarningCode,
    pub paths: Vec<ProjectPath>,
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
    /// Digest of the ordered directory and file operation projection.
    pub operation_digest: ObjectId,
    /// Bound after-state identity. Direct unit projections without a canonical
    /// root leave this null; engine-produced reports always carry it.
    pub prepared_after: Option<ObjectId>,
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
            operation_digest: crate::prepared_after::operations(change)?,
            prepared_after: None,
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

    /// Project a runtime bundle, including the canonical-root-bound
    /// prepared-after identity used by verification and portable plans.
    pub fn of_bundle(bundle: &crate::pipeline::PreparedBundle) -> Result<Self> {
        let mut report = Self::of(&bundle.change)?;
        report.prepared_after = Some(crate::prepared_after::digest(&bundle.root, &bundle.change)?);
        Ok(report)
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
        path: directory.path().clone(),
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
pub(crate) fn render(report: &Report) -> String {
    let mut out = format!("plan {} {}\n", report.transaction, kind_label(&report.kind));
    for operation in &report.operations {
        out.push_str(&format!(
            "  {:<7} {}\n",
            operation.kind.verb(),
            operation.path
        ));
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

pub(crate) fn kind_label(kind: &PreparedKind) -> &'static str {
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
        PostCommitEffect::ApplyMigrations {
            datasource,
            migrations,
        } => format!(
            "apply {} migration(s) to datasource {}",
            migrations.len(),
            datasource.as_str()
        ),
    }
}

/// The one-line summary, which has to be able to say "nothing".
///
/// A run that found everything already in place is a real outcome, and
/// printing a confident "applied" over it is how a tool teaches people to
/// stop reading its output.
///
/// **Nothing calls it.** `render_envelope` below also says "nothing to do",
/// but for a different case -- no report, no receipt and no error at all --
/// so the distinction this draws, *a prepared Apply whose plan turned out to
/// be empty*, is one no command currently makes. `pending.md` §7.2 is what
/// surfaced that; the fix is a call site, not a deletion.
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

/// Human-only timing expansion used by `--debug`.
pub fn render_timings(timings: &[crate::timing::TimingSpan]) -> String {
    timings
        .iter()
        .map(|span| {
            format!(
                "  timing  {:<9} {} us\n",
                span.phase.label(),
                span.duration_micros
            )
        })
        .collect()
}

/// The same envelope, as one JSON object.
///
/// **One projection, two encodings.** A preview and a commit produce the same
/// keys -- `status`, the operation list, the ledger line, the effects -- from
/// whichever half the envelope carries, because §R3.4's whole point is that a
/// dry run and the run that follows it describe the work in one vocabulary.
/// The obvious alternative, a JSON shape per report type, is how `--pretend`
/// came to call a replace an `update` in the first place.
///
/// Hand-written, like every other `--json` in this tool: clap is the only
/// dependency, and a serialiser earns its place when something has to *read*
/// JSON, which nothing here does.
pub fn render_envelope_json(envelope: &crate::command::CommandEnvelope) -> String {
    render_envelope_json_with_review(envelope, None)
}

/// The canonical JSON envelope with optional, explicitly requested review
/// fields. With no review this is byte-for-byte [`render_envelope_json`]; the
/// transaction result remains the same value and `diffs`/`ast` are additional
/// views over the prepared bytes and semantic edits.
pub fn render_envelope_json_with_review(
    envelope: &crate::command::CommandEnvelope,
    review: Option<(
        &crate::review::PreparedReview,
        crate::review::ReviewSelection,
    )>,
) -> String {
    use jails_support::json;

    let (transaction, kind, operations, ledger, effects, warnings) =
        match (&envelope.report, &envelope.receipt) {
            (Some(crate::command::CommandReport::Prepared(report)), _) => (
                Some(report.transaction.to_string()),
                Some(kind_label(&report.kind).to_string()),
                report
                    .operations
                    .iter()
                    .map(|op| (op.kind.label().to_string(), target_json(&op.path)))
                    .collect::<Vec<_>>(),
                ledger_label(report.ledger.kind),
                report
                    .post_commit
                    .iter()
                    .map(|effect| {
                        (
                            effect_label(&effect.effect),
                            state_label(&effect.state).to_string(),
                        )
                    })
                    .collect::<Vec<_>>(),
                report
                    .warnings
                    .iter()
                    .map(|warning| (warning.code.label().to_string(), warning.message.clone()))
                    .collect::<Vec<_>>(),
            ),
            (Some(crate::command::CommandReport::EffectRetry(retry)), _) => (
                Some(retry.transaction.to_string()),
                Some("effect-retry".to_string()),
                Vec::new(),
                None,
                vec![(retry.effect_id.to_string(), "pending".to_string())],
                Vec::new(),
            ),
            (None, Some(receipt)) => (
                Some(receipt.transaction_id.to_string()),
                Some(receipt.outcome.label().to_string()),
                receipt
                    .directories
                    .iter()
                    .map(|directory| {
                        (
                            ReportedOpKind::CreateDirectory.label().to_string(),
                            format!(
                                "{{\"kind\": \"project\", \"path\": {}}}",
                                json::string(&directory.path.to_string())
                            ),
                        )
                    })
                    .chain(
                        receipt
                            .files
                            .iter()
                            .map(|file| (verb_label(file).to_string(), target_json(&file.path))),
                    )
                    .collect::<Vec<_>>(),
                ledger_label(ledger_of(receipt.ledger_before, receipt.ledger_after).kind),
                receipt
                    .post_commit
                    .iter()
                    .map(|effect| {
                        (
                            effect_label(&effect.effect),
                            state_label(&effect.state).to_string(),
                        )
                    })
                    .collect::<Vec<_>>(),
                Vec::new(),
            ),
            (None, None) => (None, None, Vec::new(), None, Vec::new(), Vec::new()),
        };

    let operations = operations
        .iter()
        .map(|(verb, target)| {
            format!(
                "    {{\"kind\": {}, \"target\": {target}}}",
                json::string(verb)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let effects = effects
        .iter()
        .map(|(effect, state)| {
            format!(
                "    {{\"effect\": {}, \"state\": {}}}",
                json::string(effect),
                json::string(state)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let warnings = warnings
        .iter()
        .map(|(code, message)| {
            format!(
                "    {{\"code\": {}, \"message\": {}}}",
                json::string(code),
                json::string(message)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let recovery = envelope
        .recovery
        .iter()
        .flat_map(|outcome| outcome.changes.iter())
        .map(|change| format!("    {}", json::string(&recovery_line(change))))
        .collect::<Vec<_>>()
        .join(",\n");
    let error = match &envelope.error {
        Some(error) => format!(
            "{{\"code\": {}, \"message\": {}}}",
            json::string(error.code.label()),
            json::string(&error.message)
        ),
        None => "null".to_string(),
    };
    let timings = envelope
        .timings
        .iter()
        .map(|span| {
            format!(
                "    {{\"phase\": {}, \"duration_micros\": {}}}",
                json::string(span.phase.label()),
                json::string(&span.duration_micros.to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let review_fields = review
        .filter(|(_, selection)| selection.any())
        .map(|(review, selection)| crate::review::render_json_fields(review, selection))
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| format!(",\n  {}: {value}", json::string(name)))
        .collect::<String>();

    format!(
        "{{\n  \"schema_version\": 1,\n  \"status\": {},\n  \"project_commit\": {},\n  \"transaction\": {},\n  \"kind\": {},\n  \"ledger\": {},\n  \"recovery\": [{}],\n  \"operations\": [{}],\n  \"effects\": [{}],\n  \"warnings\": [{}],\n  \"error\": {error},\n  \"timings\": [{}]{review_fields}\n}}",
        json::string(envelope.status.label()),
        json::string(envelope.project_commit.label()),
        json::optional_string(transaction.as_deref()),
        json::optional_string(kind.as_deref()),
        json::optional_string(ledger.as_deref()),
        wrap(&recovery),
        wrap(&operations),
        wrap(&effects),
        wrap(&warnings),
        wrap(&timings),
    )
}

/// A JSON array body on its own lines, or nothing at all when it is empty.
///
/// `[]` rather than `[\n\n  ]`: an empty list is the common case in this
/// output and a reader scanning for what happened should not have to skip
/// blank brackets to find it.
fn wrap(rows: &str) -> String {
    match rows.is_empty() {
        true => String::new(),
        false => format!("\n{rows}\n  "),
    }
}

/// A target is always a project path, and says so.
///
/// The `kind` key survives the one thing that used to vary -- machine state
/// being retired by a schema-1 migration, which no longer exists -- because a
/// consumer reading `operations[].path.kind` should not have to change when
/// something else is added beside it.
fn target_json(target: &ProjectPath) -> String {
    format!(
        "{{\"kind\": \"project\", \"path\": {}}}",
        jails_support::json::string(&target.to_string())
    )
}

fn ledger_label(kind: ReportedLedgerKind) -> Option<String> {
    match kind {
        ReportedLedgerKind::Unchanged => None,
        other => Some(other.label().to_string()),
    }
}

fn verb_label(file: &crate::receipt::FileReceipt) -> &'static str {
    match (&file.before, &file.after) {
        (FileImage::Absent, _) => ReportedOpKind::Create.label(),
        (_, FileImage::Absent) => ReportedOpKind::Delete.label(),
        _ => ReportedOpKind::Replace.label(),
    }
}

/// What a commit did, in the same shape as what a plan would have done.
pub(crate) fn render_receipt(receipt: &AppliedReceipt) -> String {
    let mut out = format!("{} {}\n", receipt.outcome.label(), receipt.transaction_id);
    for directory in &receipt.directories {
        out.push_str(&format!("  {:<7} {}\n", "mkdir", directory.path));
    }
    for file in &receipt.files {
        out.push_str(&format!("  {:<7} {}\n", verb_of(file), file.path));
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

pub fn recovery_line(change: &crate::recovery::RecoveryChange) -> String {
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

    #[test]
    fn human_and_json_envelopes_match_the_protocol_golden() {
        let report = Report::of(&change_with(vec![create(
            "src/main/java/com/example/Note.java",
            b"package com.example;\n\npublic record Note(String title) {}\n",
        )]))
        .unwrap();
        let envelope = crate::command::CommandEnvelope::preview(report);
        let actual = format!(
            "=== human ===\n{}=== json ===\n{}\n",
            render_envelope(&envelope),
            render_envelope_json(&envelope)
        );
        let expected = include_str!("../../../tests/protocol-golden/command-envelope.txt");
        assert_eq!(actual, expected);
    }

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
            .map(|op| op.path.to_string())
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

    #[test]
    fn warnings_sort_by_code_then_path_then_message() {
        let mut report = Report::of(&change_with(Vec::new())).unwrap();
        report.warn(Warning {
            code: WarningCode::PostCommitDeferred,
            paths: Vec::new(),
            message: "the compose reconcile has not been attempted".to_string(),
        });
        report.warn(Warning {
            code: WarningCode::UnmanagedRetained,
            paths: Vec::new(),
            message: "kept two hand-written properties".to_string(),
        });
        assert_eq!(report.warnings[0].code, WarningCode::UnmanagedRetained);
    }
}
