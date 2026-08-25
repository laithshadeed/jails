//! The order a project must be read in, enforced by the type system.
//!
//! ## Why the order is not a convention
//!
//! plan.md §R2.2: *"It also cannot parse ordinary bootstrap files before
//! checking the ledger: a valid committed conflict may intentionally leave a
//! POM, human config, manifest or source file containing markers."*
//!
//! That is the whole hazard. A stored conflict is a *correct* state, and the
//! files it left behind contain `<<<<<<<` in the middle of XML, TOML, YAML and
//! Java. Handing those to the ordinary parsers does not produce an error —
//! it produces a confident wrong answer, which is then planned against.
//!
//! A comment saying "check the ledger first" is a rule that decays. So the
//! permission to run the ordinary parsers is a *value*, and the only way to
//! obtain one is to have classified the ledger and found no pending conflict.
//! The unsafe path is unreachable rather than discouraged.
//!
//! ```text
//! begin(root, machine root)
//!   -> with_ledger(...)       // strictly decoded, nothing else parsed yet
//!   -> classify()
//!        |- Pending(..)       // frozen record only; no ordinary parser exists
//!        `- Ready(OrdinaryBootstrap)  // the permission, and the only one
//! ```
//!
//! ## Nothing calls this yet
//!
//! Closing this crate's API to `pub(crate)` (`pending.md` §7.2) made that
//! visible: with `dead_code = "deny"`, 6 items here are reachable from
//! nothing. They are `pub` for that reason and no other. This is not stale
//! code -- it is encoded, round-tripped and unit-tested -- it is `pending.md`
//! §11's "conflicted merges cannot be resumed", which lands as one piece or
//! not at all: the frozen record, the refusal while it stands, and the
//! continue/abort commands. Building only the enter side was tried and backed
//! out, so a project that can enter a conflicted state and not leave it is
//! exactly what these types must not be wired up to produce.

use crate::Result;
use crate::envelope::{LedgerV2, PendingMarker};
use crate::request::RequestSyntaxFingerprint;
use crate::snapshot::{CanonicalRoot, MachineRootPresence};

/// A project whose ledger has been read and nothing else.
#[derive(Clone, Debug)]
pub struct Bootstrap {
    root: CanonicalRoot,
    machine_root: MachineRootPresence,
    ledger: Option<LedgerV2>,
    ledger_seen: bool,
}

impl Bootstrap {
    /// Step 1 and 2: the resolved root, and whether `.jails` is there at all.
    pub fn begin(root: CanonicalRoot, machine_root: MachineRootPresence) -> Self {
        Self {
            root,
            machine_root,
            ledger: None,
            ledger_seen: false,
        }
    }

    /// Step 2, continued: the strictly decoded ledger, or its absence.
    ///
    /// `None` means the file was not there — which is different from an empty
    /// one, and different again from one that failed to decode. A decode
    /// failure never reaches here: it is an error at the call site, because a
    /// ledger jails cannot read is not a project jails owns nothing in.
    pub fn with_ledger(mut self, ledger: Option<LedgerV2>) -> Result<Self> {
        if self.ledger_seen {
            return Err(jails_support::Failure::Told(
                "the ledger was supplied twice to one bootstrap".to_string(),
            ));
        }
        if matches!(self.machine_root, MachineRootPresence::Absent) && ledger.is_some() {
            return Err(jails_support::Failure::Told(
                "a ledger was decoded although `.jails` is recorded absent; the two \
                 observations disagree"
                    .to_string(),
            ));
        }
        self.ledger = ledger;
        self.ledger_seen = true;
        Ok(self)
    }

    /// Step 5: the fork. This is the only way to obtain permission to run the
    /// ordinary parsers.
    pub fn classify(self) -> Result<LoadedProject> {
        if !self.ledger_seen {
            return Err(jails_support::Failure::Told(
                "classify() was called before the ledger was read.\n       fix: the ledger \
                 decides whether the ordinary parsers are safe to run at all."
                    .to_string(),
            ));
        }
        match self
            .ledger
            .as_ref()
            .and_then(|l| l.pending_conflict.clone())
        {
            Some(marker) => Ok(LoadedProject::Pending(PendingBootstrap {
                root: self.root,
                marker,
                ledger: self.ledger.expect("a marker came from a ledger"),
            })),
            None => Ok(LoadedProject::Ready(OrdinaryBootstrap {
                root: self.root,
                ledger: self.ledger,
            })),
        }
    }
}

/// Which of the two shapes a project turned out to be in.
#[derive(Clone, Debug)]
pub enum LoadedProject {
    /// No stored conflict: ordinary bootstrap parsing may proceed.
    Ready(OrdinaryBootstrap),
    /// A conflict is stored. The ordinary parsers must never run, because the
    /// files they would read may contain markers on purpose.
    Pending(PendingBootstrap),
}

impl LoadedProject {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }
}

/// Permission to run the ordinary parsers, and the proof it was earned.
///
/// There is no constructor. The only way to hold one is to have classified a
/// ledger with no pending conflict, which is what makes "check the ledger
/// first" a fact about the program rather than a note in a comment.
#[derive(Clone, Debug)]
pub struct OrdinaryBootstrap {
    root: CanonicalRoot,
    ledger: Option<LedgerV2>,
}

impl OrdinaryBootstrap {
    pub fn root(&self) -> &CanonicalRoot {
        &self.root
    }

    /// The observed state. `None` is a project jails has never written to.
    pub fn ledger(&self) -> Option<&LedgerV2> {
        self.ledger.as_ref()
    }
}

/// A project stopped mid-reconciliation.
#[derive(Clone, Debug)]
pub struct PendingBootstrap {
    root: CanonicalRoot,
    marker: PendingMarker,
    ledger: LedgerV2,
}

impl PendingBootstrap {
    pub fn root(&self) -> &CanonicalRoot {
        &self.root
    }

    pub fn marker(&self) -> &PendingMarker {
        &self.marker
    }

    pub fn ledger(&self) -> &LedgerV2 {
        &self.ledger
    }

    /// What the reader should be told, and what they can do next.
    pub fn report(&self) -> String {
        format!(
            "this project has an unfinished change with conflicts still in the tree.\n       \
             {}\n       fix: resolve the conflict markers and rerun the same command to \
             finish, or rerun it with `--abort-conflict` to undo it.",
            self.marker.resume_display
        )
    }

    /// Whether a rerun is the command that stalled.
    ///
    /// Recomputed from the current CLI, never from the project: the project is
    /// exactly what is in an uncertain state, and re-deriving a default from a
    /// marker-bearing file would feed conflict markers into the comparison.
    pub fn resumes(&self, current: &RequestSyntaxFingerprint) -> bool {
        &self.marker.request_syntax == current
    }

    /// The refusal for a *different* command while a conflict is stored.
    ///
    /// Deterministic and specific: running something else now would plan
    /// against half-applied state and leave two unfinished changes.
    pub fn refuse_other(&self) -> String {
        format!(
            "a different unfinished change is stored, so this command cannot run.\n       {}\n\
             \x20      fix: finish it by rerunning that command, or abandon it with \
             `--abort-conflict`, before starting another.",
            self.marker.resume_display
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::CanonicalRequestSyntaxV1;
    use std::collections::{BTreeMap, BTreeSet};

    fn root() -> CanonicalRoot {
        CanonicalRoot::new("/srv/demo").unwrap()
    }

    fn fingerprint(command: &str) -> RequestSyntaxFingerprint {
        CanonicalRequestSyntaxV1 {
            command_path: vec![command.to_string()],
            positionals: Vec::new(),
            options: BTreeMap::new(),
            flags: BTreeSet::new(),
        }
        .fingerprint()
        .unwrap()
    }

    fn ledger(pending: Option<PendingMarker>) -> LedgerV2 {
        LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 3,
            last_operation: None,
            applied: Vec::new(),
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            pending_conflict: pending,
        }
    }

    fn marker(command: &str) -> PendingMarker {
        PendingMarker {
            operation: crate::identity::OperationId::from_bytes(jails_support::codec::sha256(
                b"op",
            )),
            generation: 3,
            request_syntax: fingerprint(command),
            resume_display: "jails app apply".to_string(),
        }
    }

    /// The hazard in one test. A stored conflict is a *correct* state whose
    /// files contain markers on purpose, so the ordinary parsers must be
    /// unreachable — not merely discouraged.
    #[test]
    fn a_pending_conflict_yields_no_permission_to_parse_ordinary_files() {
        let loaded = Bootstrap::begin(root(), MachineRootPresence::Present)
            .with_ledger(Some(ledger(Some(marker("apply")))))
            .unwrap()
            .classify()
            .unwrap();

        assert!(loaded.is_pending());
        match loaded {
            LoadedProject::Pending(pending) => {
                // The only things reachable are the frozen record and the
                // report. There is no `OrdinaryBootstrap` to be had.
                assert_eq!(pending.marker().generation, 3);
                let report = pending.report();
                assert!(report.contains("conflict markers"), "{report}");
                assert!(report.contains("--abort-conflict"), "{report}");
            }
            LoadedProject::Ready(_) => panic!("a pending ledger must not yield Ready"),
        }
    }

    #[test]
    fn a_clean_ledger_yields_the_permission() {
        let loaded = Bootstrap::begin(root(), MachineRootPresence::Present)
            .with_ledger(Some(ledger(None)))
            .unwrap()
            .classify()
            .unwrap();
        match loaded {
            LoadedProject::Ready(ready) => {
                assert_eq!(ready.root().as_str(), "/srv/demo");
                assert_eq!(ready.ledger().unwrap().generation, 3);
            }
            LoadedProject::Pending(_) => panic!("no conflict is stored"),
        }
    }

    /// A project jails has never written to is `Ready` with no observed state,
    /// which is different from one whose ledger failed to decode — that never
    /// reaches here at all.
    #[test]
    fn an_absent_ledger_is_ready_with_nothing_observed() {
        let loaded = Bootstrap::begin(root(), MachineRootPresence::Absent)
            .with_ledger(None)
            .unwrap()
            .classify()
            .unwrap();
        match loaded {
            LoadedProject::Ready(ready) => assert!(ready.ledger().is_none()),
            LoadedProject::Pending(_) => panic!("nothing is stored"),
        }
    }

    /// Two observations of the same thing that disagree are a bug, not a
    /// precedence question.
    #[test]
    fn a_ledger_without_a_machine_root_refuses() {
        let error = Bootstrap::begin(root(), MachineRootPresence::Absent)
            .with_ledger(Some(ledger(None)))
            .unwrap_err();
        assert!(error.contains("disagree"), "{error}");
    }

    /// Classifying is the gate, so skipping it cannot be an accident.
    #[test]
    fn classifying_before_reading_the_ledger_refuses() {
        let error = Bootstrap::begin(root(), MachineRootPresence::Present)
            .classify()
            .unwrap_err();
        assert!(error.contains("before the ledger was read"), "{error}");
    }

    /// The project is exactly what is uncertain, so the comparison is against
    /// the *current CLI* and never against a default re-derived from a
    /// marker-bearing file.
    #[test]
    fn a_rerun_is_recognised_by_request_and_a_different_command_is_refused() {
        let loaded = Bootstrap::begin(root(), MachineRootPresence::Present)
            .with_ledger(Some(ledger(Some(marker("apply")))))
            .unwrap()
            .classify()
            .unwrap();
        let LoadedProject::Pending(pending) = loaded else {
            panic!("pending");
        };

        assert!(pending.resumes(&fingerprint("apply")));
        assert!(!pending.resumes(&fingerprint("sync")));

        let refusal = pending.refuse_other();
        assert!(refusal.contains("different unfinished change"), "{refusal}");
        assert!(refusal.contains("--abort-conflict"), "{refusal}");
    }

    #[test]
    fn a_ledger_may_not_be_supplied_twice() {
        let error = Bootstrap::begin(root(), MachineRootPresence::Present)
            .with_ledger(Some(ledger(None)))
            .unwrap()
            .with_ledger(Some(ledger(None)))
            .unwrap_err();
        assert!(error.contains("twice"), "{error}");
    }

    /// The marker survives the ledger's own round trip, so a resume after a
    /// restart still recognises its command.
    #[test]
    fn a_pending_marker_round_trips_through_the_ledger_file() {
        let original = ledger(Some(marker("apply")));
        let source = original.render().unwrap();
        let back = LedgerV2::parse_file(&source).unwrap();
        assert_eq!(back, original);
        assert_eq!(
            back.pending_conflict.as_ref().unwrap().request_syntax,
            fingerprint("apply")
        );
    }

    /// Generation zero would make "never committed" and "committed once" the
    /// same recorded value, in a record a resume depends on.
    #[test]
    fn a_pending_marker_with_generation_zero_refuses() {
        let mut broken = marker("apply");
        broken.generation = 0;
        assert!(ledger(Some(broken)).encode().is_err());
    }
}
