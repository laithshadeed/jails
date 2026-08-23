//! Convergence under interruption.
//!
//! plan.md §R4.5's property, stated once: whatever instant a run stops at,
//! running it again reaches a consistent state — the transaction is either
//! completely applied and published, or completely absent, and there is never
//! a half-applied tree with nothing to say so.
//!
//! Each test arms one failpoint, runs a commit, then runs recovery twice and
//! asserts the result. Twice, because recovery that converges on the first
//! pass and changes something on the second is not idempotent, and that
//! difference only shows up under a second crash.
//!
//! What this does not model is losing stack cleanup — that needs a child
//! process and `abort()`, which needs the CLI to route through this executor.
//! §R4.5's child-abort suite lands with R6's migration; the failpoint set is
//! the same one.

use jails_commit::execute::{LockedProject, ProjectHandle, commit};
use jails_commit::fault::{Armed, POINTS};
use jails_commit::journal::ReceiptV1;
use jails_commit::outcome::{CommitResult, RecoveryError};
use jails_commit::recover::recover_locked;
use jails_prepare::operation::{ApplySemantics, OperationIdentityV1, OperationSemanticsV1};
use jails_prepare::pipeline::PreparedBundle;
use jails_prepare::prepare::{DirectoryOp, FileOp, OperationTarget, PreparedChange, PreparedKind};
use jails_prepare::tool::{OperationContextFingerprint, PreparationContextFingerprint};
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::identity::{ObjectId, ObjectRef, ProjectPath, TransactionId};
use jails_protocol::plan::{LedgerIntent, PlannedSubject};
use jails_protocol::snapshot::CanonicalRoot;
use jails_support::codec::sha256;
use jails_support::scratch::ScratchDir;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const BODY: &[u8] = b"class App {}\n";
const AT: &str = "src/main/java/App.java";

fn one_create() -> PreparedChange {
    let after = ObjectRef::new(ObjectId::from_bytes(sha256(BODY)), BODY.len() as u64);
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
                legacy_after: Vec::new(),
            },
            migration: None,
        })),
    };
    let mut change = PreparedChange {
        operation_id: operation_identity.operation_id().unwrap(),
        operation_identity,
        transaction_id: TransactionId::from_bytes([0; 32]),
        preparation: PreparationContextFingerprint::default(),
        input_preconditions: Vec::new(),
        operations: vec![FileOp::Create {
            path: OperationTarget::Project(ProjectPath::parse(AT).unwrap()),
            after,
            mode: FileMode::new(0o644).unwrap(),
            contributors: BTreeSet::new(),
        }],
        directories: ["src", "src/main", "src/main/java"]
            .into_iter()
            .map(|path| DirectoryOp::Create {
                path: ProjectPath::parse(path).unwrap(),
            })
            .collect(),
        ledger_before: FileImage::Absent,
        ledger_after: FileImage::Absent,
        objects: BTreeMap::from([(ObjectId::from_bytes(sha256(BODY)), Arc::from(BODY.to_vec()))]),
        post_commit: Vec::new(),
        kind: PreparedKind::Apply,
    };
    change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
    change
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
    }
}

fn project() -> (ScratchDir, LockedProject) {
    let scratch = ScratchDir::in_temp("jails-crash").unwrap();
    let handle = ProjectHandle::at(scratch.path()).unwrap();
    let locked = LockedProject::acquire(handle, "crash suite").unwrap();
    (scratch, locked)
}

/// Where a project ended up, in the two terms that matter.
#[derive(Debug, Eq, PartialEq)]
struct Settled {
    file_present: bool,
    receipt_published: bool,
}

fn settle(scratch: &ScratchDir, locked: &LockedProject, transaction: &TransactionId) -> Settled {
    Settled {
        file_present: scratch.path().join(AT).exists(),
        receipt_published: locked.handle().store().receipt(transaction).exists(),
    }
}

fn recover_twice(locked: &LockedProject) -> std::result::Result<(), RecoveryError> {
    recover_locked(locked)?;
    let second = recover_locked(locked)?;
    assert!(
        second.is_clean(),
        "the second recovery pass changed something: {second:?}"
    );
    Ok(())
}

#[test]
fn every_named_failpoint_converges() {
    for point in POINTS {
        let (scratch, locked) = project();
        let change = one_create();
        let transaction = change.transaction_id;

        let interrupted = {
            let _armed = Armed::at(point);
            commit(&locked, &bundle(&locked, change))
        };

        if let Err(error) = recover_twice(&locked) {
            panic!("`{point}` left a project recovery could not settle: {error}");
        }

        let settled = settle(&scratch, &locked, &transaction);
        let applied = Settled {
            file_present: true,
            receipt_published: true,
        };
        let absent = Settled {
            file_present: false,
            receipt_published: false,
        };
        assert!(
            settled == applied || settled == absent,
            "`{point}` settled at {settled:?} (commit returned {interrupted:?})"
        );

        let staging: Vec<_> = std::fs::read_dir(locked.handle().store().transactions())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert!(staging.is_empty(), "`{point}` left staging: {staging:?}");

        scratch.close().unwrap();
    }
}

/// The instant that divides the protocol, from both sides.
#[test]
fn a_failure_before_the_ledger_refuses_and_one_after_it_is_committed() {
    let (scratch, locked) = project();
    let change = one_create();
    let transaction = change.transaction_id;
    {
        let _armed = Armed::at("before-ledger");
        commit(&locked, &bundle(&locked, change)).unwrap_err();
    }
    recover_twice(&locked).unwrap();
    // The transaction had activated, so recovery finishes it forward.
    assert!(locked.handle().store().receipt(&transaction).exists());
    scratch.close().unwrap();

    let (scratch, locked) = project();
    let change = one_create();
    let transaction = change.transaction_id;
    let result = {
        let _armed = Armed::at("after-ledger-rename");
        commit(&locked, &bundle(&locked, change)).unwrap()
    };
    // Past the commit point, so this is a success-side value: the work is
    // durable and the caller must not be told to retry it.
    assert!(
        matches!(result, CommitResult::CommittedRecoveryRequired(_)),
        "{result:?}"
    );
    recover_twice(&locked).unwrap();
    ReceiptV1::read(&locked.handle().store().receipt(&transaction)).unwrap();
    scratch.close().unwrap();
}

/// A crash after activation, then a user edit to the very file the
/// transaction was going to write. Recovery must not overwrite it.
#[test]
fn recovery_never_overwrites_a_file_it_does_not_recognise() {
    let (scratch, locked) = project();
    let change = one_create();
    {
        let _armed = Armed::at("after-journal-active");
        commit(&locked, &bundle(&locked, change)).unwrap_err();
    }

    std::fs::create_dir_all(scratch.path().join("src/main/java")).unwrap();
    std::fs::write(scratch.path().join(AT), b"class App { /* mine */ }\n").unwrap();

    let _ = recover_locked(&locked);
    assert_eq!(
        std::fs::read(scratch.path().join(AT)).unwrap(),
        b"class App { /* mine */ }\n",
        "recovery overwrote a file it did not recognise"
    );
    scratch.close().unwrap();
}

/// A plan prepared for one project may not be committed to another.
///
/// Not a hypothetical: every path in a prepared operation is project-relative,
/// so a bundle for a same-shaped project passes every precondition against the
/// wrong tree and writes it. The bundle has always carried the root it was
/// prepared against; `commit` compares it now.
#[test]
fn a_plan_prepared_elsewhere_is_refused_before_anything_is_written() {
    let (theirs, locked) = project();
    let elsewhere = ScratchDir::in_temp("jails-crash-elsewhere").unwrap();
    let mut change = one_create();
    change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
    let foreign = PreparedBundle {
        root: CanonicalRoot::new(
            std::fs::canonicalize(elsewhere.path())
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap(),
        change,
    };

    let error = commit(&locked, &foreign).unwrap_err();
    assert!(
        format!("{error:?}").contains("prepared for"),
        "the refusal names both projects: {error:?}"
    );
    assert!(
        !theirs.path().join(AT).exists(),
        "and nothing of the foreign plan reached this project"
    );
}
