//! Convergence under interruption, for the one canonical executor.
//!
//! The executor trades rollback away for convergence: *a crashed command may
//! leave a temporarily mixed but individually valid tree; the next identical
//! generation repairs it deterministically*. That sentence is the whole safety
//! story of `jails-workspace::execute`, and this file is its proof.
//!
//! There is no journal to roll forward and no preimage to roll back. What is
//! asserted is:
//!
//! 1. every advertised failpoint is actually reached, so no row of the matrix
//!    silently proves nothing;
//! 2. re-running *the same bundle* after a death there reaches byte-for-byte
//!    the tree a clean run reaches;
//! 3. a further run writes nothing and deletes nothing, so convergence is
//!    idempotent rather than merely eventual.
//!
//! (3) is the half a "the second run fixes it" claim usually skips. An
//! executor that rewrote every file on every run would satisfy (2) forever
//! and still be unable to tell a reader whether anything had changed.
//!
//! `every_failpoint_converges_after_a_child_dies_there` repeats the matrix in
//! a child process that `abort()`s inside the trip. The in-process half
//! injects an `Err`, which unwinds -- so the flock is released in order, the
//! staged `NamedTempFile` is cleaned up by its guard, and buffers flush. A
//! machine losing power does none of that, and a convergence proof built only
//! on the unwinding case has proved the easier half.

use jails_contracts::{CanonicalModelPatch, ModelFileUpdate, PlanBundle, ProjectPath};
use jails_workspace::fault::{Armed, POINTS};
use std::collections::BTreeMap;
use std::path::Path;

/// The starting model. Two entities, so the next one can drop one.
const BEFORE: &str = "\
jdl 1
app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform plain
  build maven
  storage none
}

entity Note @id(ent_note) {
  id: uuid @id(fld_note_id) @pk
  title: string @id(fld_note_title)
}

entity Memo @id(ent_memo) {
  id: uuid @id(fld_memo_id) @pk
  body: string @id(fld_memo_body)
}
";

/// The model under test. `Memo` is gone, so applying this prunes managed
/// files -- which is the only way `before-remove` and `after-remove` are
/// reachable at all.
const AFTER: &str = "\
jdl 1
app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform plain
  build maven
  storage none
}

entity Note @id(ent_note) {
  id: uuid @id(fld_note_id) @pk
  title: string @id(fld_note_title)
  summary: string @id(fld_note_summary)
}
";

const POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>notes</artifactId>
  <version>0.0.1-SNAPSHOT</version>
  <dependencies>
  </dependencies>
</project>
"#;

const MODEL_PATH: &str = ".jails/model.jdl";

/// A bare project the first plan can be applied to.
fn seed(root: &Path) {
    std::fs::create_dir_all(root.join(".jails")).unwrap();
    std::fs::write(root.join("pom.xml"), POM).unwrap();
    std::fs::write(root.join(MODEL_PATH), BEFORE).unwrap();
}

/// Capture, compile and materialize one model edit against the tree.
///
/// This is the whole canonical pipeline minus the executor, which is what
/// makes the bundle a *real* one rather than a hand-built fixture: its
/// preconditions are captured from the tree the crash will damage.
///
/// `current` is what is on disk and `next` is what the plan installs, exactly
/// as `model_generate` splits them -- the capture has to describe the tree
/// being changed, not the one being asked for.
fn bundle_for(root: &Path, current: &str, next: &str) -> PlanBundle {
    let model = jails_model::parse_jdl(next).expect("the fixture model parses");
    let snapshot =
        jails_workspace::capture(root, Path::new(MODEL_PATH), current.as_bytes(), model, &[])
            .expect("the fixture project captures");
    let draft = jails_compiler::Compiler::compile(&snapshot, None).expect("the model compiles");
    jails_workspace::materialize(
        &snapshot,
        CanonicalModelPatch::reconcile(),
        draft,
        Some(ModelFileUpdate {
            retire: Vec::new(),
            path: ProjectPath::parse(MODEL_PATH).unwrap(),
            bytes: next.as_bytes().to_vec(),
        }),
        jails_compiler::COMPILER_VERSION,
        jails_workspace::Restore::Refuse,
    )
    .expect("the draft materializes")
}

/// Apply `BEFORE`, so a project with `Memo` in it exists to prune.
fn applied_before(root: &Path) {
    seed(root);
    let bundle = bundle_for(root, BEFORE, BEFORE);
    jails_workspace::execute(root, &bundle).expect("the first plan applies cleanly");
}

/// Every file under `root`, so two trees can be compared byte for byte.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

fn walk(root: &Path, at: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(at).expect("the project is readable") {
        let entry = entry.expect("the project is readable");
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, into);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("walked from the root")
                .to_string_lossy()
                .replace('\\', "/");
            into.insert(
                relative,
                std::fs::read(&path).expect("the file is readable"),
            );
        }
    }
}

/// The tree a clean run reaches, which is what every crashed run must reach.
fn reference() -> BTreeMap<String, Vec<u8>> {
    let scratch = jails_support::scratch::ScratchDir::in_temp("jails-workspace-crash-ref").unwrap();
    applied_before(scratch.path());
    let bundle = bundle_for(scratch.path(), BEFORE, AFTER);
    jails_workspace::execute(scratch.path(), &bundle).expect("the second plan applies cleanly");
    tree(scratch.path())
}

/// Every advertised failpoint, with an injected `Err` at each.
///
/// The `is_err` assertion is the one that keeps the matrix honest. Without it
/// a point nothing reaches would sail through the convergence checks -- they
/// would be comparing a clean run against a clean run -- and report a pass
/// over a path the executor never took.
#[test]
fn every_named_failpoint_converges() {
    let want = reference();
    for point in POINTS {
        let scratch = jails_support::scratch::ScratchDir::in_temp("jails-workspace-crash").unwrap();
        let root = scratch.path();
        applied_before(root);
        let bundle = bundle_for(root, BEFORE, AFTER);

        {
            let _armed = Armed::at(point);
            let interrupted = jails_workspace::execute(root, &bundle);
            assert!(
                interrupted.is_err(),
                "`{point}` never tripped -- the plan ran to completion, so this \
                 row of the matrix proves nothing"
            );
        }

        let repaired = jails_workspace::execute(root, &bundle).unwrap_or_else(|error| {
            panic!("`{point}` left a tree the same plan could not repair: {error}")
        });
        assert_eq!(tree(root), want, "`{point}` converged on a different tree");

        let again = jails_workspace::execute(root, &bundle)
            .unwrap_or_else(|error| panic!("`{point}` was not settled after repair: {error}"));
        assert_eq!(
            (again.files_written, again.files_deleted),
            (0, 0),
            "`{point}` repaired to a state the executor still wants to change \
             (the repair reported {}/{} written/deleted)",
            repaired.files_written,
            repaired.files_deleted
        );
        assert_eq!(tree(root), want, "`{point}` moved on its second repair");
    }
}

/// The environment variable that turns this binary into the crashing child.
const CHILD_POINT: &str = "JAILS_WORKSPACE_CRASH_POINT";
/// The project the parent prepared, which the child applies `AFTER` to.
const CHILD_ROOT: &str = "JAILS_WORKSPACE_CRASH_ROOT";

/// The child half of the abort matrix. Not a test on its own.
///
/// It is a `#[test]` because that is how a test binary exposes an entry point
/// the parent can name with `--exact`. Without the environment variable it
/// returns immediately, so an ordinary run costs nothing.
#[test]
fn crash_child_executes_and_dies() {
    let Ok(point) = std::env::var(CHILD_POINT) else {
        return;
    };
    let root = std::env::var(CHILD_ROOT).expect("the parent passes both or neither");
    let root = Path::new(&root);
    let bundle = bundle_for(root, BEFORE, AFTER);
    let _armed = Armed::aborting_at(&point);
    // Reached only if the point never tripped, which is a defect in the
    // matrix rather than in the executor -- the parent reports it as one.
    let _ = jails_workspace::execute(root, &bundle);
    std::process::exit(97);
}

/// Every failpoint again, in a process that dies there without unwinding.
///
/// Each advertised fault is asserted to fire in a child process that dies
/// without unwinding; a restart must then reach the post-plan state, and a
/// second restart must be idempotent. This executor converges forward, never
/// back, so the post-plan half is the only one on offer -- and the parent
/// opens a project whose `apply.lock` is held by nobody because its owner was
/// killed mid-flock.
#[test]
fn every_failpoint_converges_after_a_child_dies_there() {
    let binary = std::env::current_exe().expect("a test binary knows its own path");
    let want = reference();
    for point in POINTS {
        let scratch =
            jails_support::scratch::ScratchDir::in_temp("jails-workspace-crash-abort").unwrap();
        let root = scratch.path().to_path_buf();
        applied_before(&root);
        // Computed *before* the child runs, on the same tree the child will
        // capture, so it is byte-identical to the bundle the child dies
        // partway through. Re-planning after the crash would capture the
        // damaged tree instead, and a plan written against the damage
        // converges trivially -- which is the property `apply never replans`
        // exists to make untrue.
        let bundle = bundle_for(&root, BEFORE, AFTER);

        let child = std::process::Command::new(&binary)
            .args(["--exact", "crash_child_executes_and_dies", "--nocapture"])
            .env(CHILD_POINT, point)
            .env(CHILD_ROOT, &root)
            .output()
            .expect("failed to spawn the crashing child");
        assert_ne!(
            child.status.code(),
            Some(97),
            "`{point}` never tripped in the child -- the plan ran to \
             completion, so this row proves nothing"
        );
        // Killed by a signal, not merely unsuccessful. A panicking child exits
        // 101 *after* unwinding -- guards released, the staged temporary
        // removed -- which is the state the in-process matrix already covers.
        // Accepting it here would make this test a slower copy of that one.
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt as _;
            assert_eq!(
                child.status.signal(),
                Some(6),
                "`{point}` did not abort the child: exit {:?}, signal {:?}\n{}",
                child.status.code(),
                child.status.signal(),
                String::from_utf8_lossy(&child.stderr)
            );
        }
        #[cfg(not(unix))]
        assert!(!child.status.success(), "`{point}` did not stop the child");

        jails_workspace::execute(&root, &bundle).unwrap_or_else(|error| {
            panic!("`{point}` left a tree the same plan could not repair after an abort: {error}")
        });
        assert_eq!(tree(&root), want, "`{point}` converged on a different tree");

        let again = jails_workspace::execute(&root, &bundle)
            .unwrap_or_else(|error| panic!("`{point}` was not settled after repair: {error}"));
        assert_eq!(
            (again.files_written, again.files_deleted),
            (0, 0),
            "`{point}` repaired to a state the executor still wants to change"
        );
    }
}
