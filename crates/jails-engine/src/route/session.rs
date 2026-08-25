//! One invocation, and the one value it returns.
//!
//! Split out of `route.rs` under abstract.md rung 11: what a run *is* -- the
//! project, whether it may write, whether it may start a container, which
//! paths it claims -- is a different secret from how a request becomes a
//! commit, and the file holding both had grown past the size the ladder's
//! largest-module gate allows.
//!
//! The two types here are a pair. `Run` is per-invocation policy, kept in one
//! value so a route cannot forget to honour `--pretend` -- it never sees the
//! flag. `Outcome` is what came back, and its `envelope` is §R3.4's one
//! command result, projected from the report or the receipt rather than
//! asserted beside them.

use super::*;

/// One run of one route: the project, and whether it may write.
///
/// A parameter object rather than a mode argument on every route, and the
/// reason is arity: `generate` already takes eight, and a ninth that most of
/// the body never mentions is exactly the shape abstract.md's first rung is
/// about. It also puts the decision in one place -- a route cannot forget to
/// honour `--pretend`, because it never sees it.
///
/// `--pretend` is not a weaker commit that stops early by luck. It runs the
/// same computation and stops one step before the lock, so what it reports is
/// the bundle the commit would have activated rather than a second
/// implementation hoping to agree with the first.
pub struct Run<'a> {
    project: &'a Project,
    pub(super) write: bool,
    /// Whether jails prints the commands it shells out to.
    ///
    /// Observability only, and it reaches the effect attempt because that is
    /// the one subprocess a mutation route runs -- a `--debug` that stopped at
    /// the file transition would go quiet exactly where a person is trying to
    /// see what happened.
    pub(super) debug: bool,
    /// Whether this invocation may start what it installs.
    ///
    /// `--no-start` is the caller declining the runtime half of a capability
    /// that brings a compose service, and it is part of *what was asked*: the
    /// canonical request carries it, so the fingerprint two invocations are
    /// compared by distinguishes `add db` from `add db --no-start`. Every
    /// route used to hardcode `no_start: false`, which made the fingerprint
    /// describe a command nobody typed.
    start: bool,
}

impl<'a> Run<'a> {
    /// A run that commits.
    pub fn committing(project: &'a Project) -> Self {
        Self {
            project,
            write: true,
            start: true,
            debug: false,
        }
    }

    /// A run that computes everything and writes nothing.
    pub fn pretending(project: &'a Project) -> Self {
        Self {
            project,
            write: false,
            start: true,
            debug: false,
        }
    }

    /// The same run, against a freshly resolved project.
    ///
    /// **`jails add db api` is two transitions, and the second has to plan
    /// against what the first wrote.** A `Project` is resolved once and holds
    /// the build file's text, so a later transition reusing it plans against a
    /// project that stopped existing one commit ago -- which is how `add db
    /// api` produced an `ApiExceptionHandler` with no `DuplicateKeyException`
    /// arm while the JDBC starter it keys off sat in the pom two lines away.
    /// `pending.md` §1.1.
    ///
    /// The flags come with it: which run this is (`committing` or
    /// `pretending`), `--debug` and `--no-start` are properties of the
    /// invocation, not of the project it is looking at.
    pub fn against<'b>(&self, project: &'b Project) -> Run<'b> {
        Run {
            project,
            write: self.write,
            debug: self.debug,
            start: self.start,
        }
    }

    /// The same run, with `--no-start`: nothing this installs is started.
    pub fn without_start(mut self) -> Self {
        self.start = false;
        self
    }

    /// The same run, printing every command it shells out to.
    pub fn with_debug(mut self) -> Self {
        self.debug = true;
        self
    }

    /// What the canonical request records, which is the caller's word for it.
    pub(super) fn no_start(&self) -> bool {
        !self.start
    }

    /// Whether this run may write. `--pretend` is the only reason it may not.
    pub fn writes(&self) -> bool {
        self.write
    }

    pub fn project(&self) -> &'a Project {
        self.project
    }
}

/// What a route did, or would have done.
///
/// One type rather than two entry points per route: the caller asked for a
/// pretend run or a real one and gets back the matching answer, so there is no
/// way to run the wrong one by picking the wrong function.
#[derive(Debug)]
pub enum Outcome {
    Committed(CommitResult),
    /// The same, plus what recovery finished on the way here.
    ///
    /// Kept as its own variant rather than a field on every outcome, because
    /// the ordinary value is empty and §R3.4 omits an observationally clean
    /// recovery entirely. A caller that sees this at all is being told an
    /// earlier interrupted run left work that this invocation completed.
    CommittedAfterRecovery(CommitResult, Vec<RecoveryOutcome>),
    /// Nothing was written. This is the prepared transition, projected.
    ///
    /// §R3.4's `Report`, not a second description of it. There used to be a
    /// hand-rolled list here, and it had already drifted from the normative
    /// projection in three ways: it called a replace an `update`, it sorted by
    /// path where the report keeps the executor's order, and it dropped
    /// directory creation entirely. A `--pretend` that describes the work in
    /// different words from the receipt is the failure the one-projection rule
    /// exists to prevent.
    Planned(Box<Report>),
}

impl Outcome {
    /// The commit, when the caller knows it asked for one.
    pub fn committed(self) -> Result<CommitResult> {
        match self {
            Self::Committed(result) | Self::CommittedAfterRecovery(result, _) => Ok(result),
            Self::Planned(_) => Err(jails_support::Failure::Told(
                "this run was asked to pretend, so there is no commit".to_string(),
            )),
        }
    }

    /// The prepared transition, for a run that planned one.
    pub fn report(&self) -> Option<&Report> {
        match self {
            Self::Planned(report) => Some(report),
            _ => None,
        }
    }

    /// The outcome recovery produced, when this attempt has to be replanned.
    ///
    /// Not an error: the caller planned against state an interrupted earlier
    /// run has since been cleaned up from, so the plan is stale and nothing is
    /// wrong. Reporting it as a failure would make an ordinary cleanup look
    /// like one.
    pub(super) fn replanned(&self) -> Option<RecoveryOutcome> {
        match self {
            Self::Committed(CommitResult::RecoveredPriorTransaction(outcome)) => {
                Some((**outcome).clone())
            }
            _ => None,
        }
    }

    /// The same outcome, carrying what recovery finished on the way.
    pub(super) fn after_recovery(self, recovery: Vec<RecoveryOutcome>) -> Self {
        match (self, recovery.is_empty()) {
            (outcome, true) => outcome,
            (Self::Committed(result), false) => Self::CommittedAfterRecovery(result, recovery),
            (Self::CommittedAfterRecovery(result, mut had), false) => {
                had.extend(recovery);
                Self::CommittedAfterRecovery(result, had)
            }
            (planned, false) => planned,
        }
    }

    /// The one value a mutation command returns, per §R3.4.
    ///
    /// Both sides are projected: a preview from the prepared report, and a
    /// commit from the receipt the executor published. The status is derived
    /// from those rather than asserted beside them, which is what stops a
    /// caller reporting an apply as a conflict or a no-op as an apply.
    ///
    /// `None` only for a commit that has not finished -- a structural failure
    /// after the ledger, which §R4.3 makes a success-side value carrying what
    /// is known, and which has no single status until §R6.8 says which.
    pub fn envelope(&self) -> Option<CommandEnvelope> {
        let (result, recovery) = match self {
            Self::Planned(report) => {
                return Some(CommandEnvelope::preview((**report).clone()));
            }
            Self::Committed(result) => (result, Vec::new()),
            Self::CommittedAfterRecovery(result, recovery) => (result, recovery.clone()),
        };
        let envelope = match result {
            CommitResult::NoOp => CommandEnvelope::no_op(),
            CommitResult::Committed(committed) => CommandEnvelope::applied(
                committed.receipt.clone(),
                ProjectCommitDisposition::Existing,
            ),
            CommitResult::RecoveredPriorTransaction(_)
            | CommitResult::CommittedRecoveryRequired(_) => return None,
        };
        Some(envelope.after_recovery(recovery))
    }

    /// The project-relative paths this outcome removed from disk.
    ///
    /// From the receipt, so it describes what a commit *did* rather than what
    /// a plan intended, and empty for a preview -- a `--pretend` that swept
    /// `target/` would be writing on a run that promised not to.
    pub fn deleted_files(&self) -> Vec<String> {
        let receipt = match self {
            Self::Committed(CommitResult::Committed(committed))
            | Self::CommittedAfterRecovery(CommitResult::Committed(committed), _) => {
                &committed.receipt
            }
            _ => return Vec::new(),
        };
        receipt
            .files
            .iter()
            .filter(|file| matches!(file.after, jails_protocol::conflict::FileImage::Absent))
            .map(|file| file.path.to_string())
            .collect()
    }

    /// Every operation a plan would perform, in the report's order.
    pub fn operations(&self) -> Vec<ReportedOp> {
        match self {
            Self::Planned(report) => report.operations.clone(),
            _ => Vec::new(),
        }
    }
}
