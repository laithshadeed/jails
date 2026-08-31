//! Does `destroy` remove exactly what `generate` wrote?
//!
//! `plan.md` §6.1 counts five separate answers to *"what files does kind X
//! produce?"*, and names the second one -- `generate::destroy`'s `match kind`
//! with its seventeen hand-written `vec![]` arms -- as the dangerous one: it
//! is a manual transcription of paths the generator right next door already
//! computes, and CLAUDE.md's warning that *"a kind added to one and not the
//! other silently strands files"* was enforced by a single test covering a
//! single kind (`Record`) out of thirty.
//!
//! This is that test, generalised over every kind the scenario table
//! exercises (§6.2 option A). It is deliberately the *evidence* that makes
//! deriving the path list from the generator (§6.2 option B) a safe change to
//! attempt: run it before and after and the sets must not move.
//!
//! Two directions, both real failures:
//!
//! - **destroy names a path generate never wrote.** A stale arm, a suffix
//!   applied on one side only, a layer renamed in one place. Harmless-looking
//!   until the path collides with something a human wrote.
//! - **generate wrote a file destroy does not remove.** A stranded file
//!   implementing a deleted interface stops the project compiling, which is
//!   exactly the `g strategy` failure the destroy arm reads disk to avoid.
//!
//! Not every leftover is a bug -- a migration is forward-only and a shared
//! registration file is still needed by the next artifact -- so leftovers are
//! matched against `ALLOWED_LEFTOVER`, whose entries each carry the reason.
//! An unexplained one fails.

mod common;

use common::scenarios::{self, SCENARIOS};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// Files a generator writes on purpose and `destroy` deliberately keeps.
///
/// Each entry is (kind or `""` for any, path fragment, why). A leftover
/// matching none of these is a stranded file, and the test names it. The
/// `why` is not decoration: an exemption without a reason is how the second
/// definition of "what kind X produces" drifted in the first place.
const ALLOWED_LEFTOVER: &[(&str, &str, &str)] = &[
    (
        "scaffold",
        ".jails/architecture.toml",
        "project-level architecture policy has its own durable owner; destroying one entity must not remove shared fitness configuration",
    ),
    (
        "scaffold",
        "ArchitectureTest.java",
        "one project-level architecture suite is shared by every scaffold and outlives any one entity",
    ),
    (
        "scaffold",
        "src/test/resources/archunit.properties",
        "project-level baseline configuration belongs to the shared architecture suite, not one scaffold",
    ),
    (
        "",
        "src/main/resources/db/migration/",
        "migrations are forward-only: destroying the Java does not un-apply a \
         migration that has already run somewhere",
    ),
    (
        "",
        "src/test/resources/fixtures/",
        "a fixture is test data a human edits, not a generated class",
    ),
    (
        "",
        "package-info.java",
        "written per package, not per artifact; the next kind in that package \
         still needs it",
    ),
    (
        "",
        "SchedulingConfig.java",
        "shared registration: a second job still needs @EnableScheduling",
    ),
    (
        "",
        "TimeOrderedUuid",
        "one time-ordered identifier per project, shared by every scaffold and \
         use case that mints a key; destroying one entity must not take the \
         generator the others call",
    ),
    (
        "",
        "pom.xml",
        "a file the user owns; generators splice into it, `remove` unsplices",
    ),
    (
        "",
        "compose.yaml",
        "a file the user owns; `remove <capability>` takes the service out",
    ),
    (
        "",
        "jails.toml",
        "the manifest; `remove` is what takes a capability back out of it",
    ),
    (
        "",
        "src/main/resources/application.properties",
        "properties live in marked blocks that `remove <capability>` unsplices",
    ),
    (
        "durable-job",
        "src/test/resources/config/application.properties",
        "a shared test property source; destroy unsplices only this durable job's marked block",
    ),
    (
        "",
        "TestcontainersConfig.java",
        "installed by `add db`, shared by every @SpringBootTest in the project",
    ),
    (
        "handler",
        "ApiError",
        "shared error body and its test: a second handler renders the same one",
    ),
    (
        "repo",
        "/domain/",
        "`g repo` lays down a *placeholder* record only when none exists, and \
         destroy cannot tell that placeholder from a record the reader wrote \
         by hand and has been editing since. Print, never clobber: the \
         Javadoc says the record is a starting point, and `destroy record` is \
         the command that removes one",
    ),
];

/// Kinds `destroy` refuses, and the refusal is the point.
///
/// Both are forward-only: a migration that has run cannot be unrun by deleting
/// its file, and a field overlay is undone by another overlay.
///
/// **Two kinds were here and are not, for the same reason.** `association`'s
/// DDL is append-only too, but retiring it *appends* `drop constraint` -- the
/// next migration, not the un-running of one -- exactly as `--storage drop`
/// appends `drop table`; refusing the verb left both halves of an association
/// permanently undestroyable. `cases` was a *ledger* limitation rather than a
/// semantic one: a one-shot was a receipt over the source's bytes and the
/// schema had no list for taking one back. On the compiler it is an ordinary
/// component declaration, so removing it is model subtraction like any other
/// and the generated test goes with it.
const FORWARD_ONLY: &[&str] = &["migration", "field"];

fn explanation(kind: &str, rel: &str) -> Option<&'static str> {
    ALLOWED_LEFTOVER
        .iter()
        .find(|(for_kind, matcher, _)| {
            (for_kind.is_empty() || *for_kind == kind) && rel.contains(matcher)
        })
        .map(|(_, _, why)| *why)
}

/// What `destroy --pretend` says it would delete, read as data.
///
/// `--output json` rather than the human rendering, and that is the point of
/// having it: a test that scrapes prose pins the prose, so a clearer message
/// becomes a test failure and the wording ossifies. The JSON is §R3.4's one
/// projection of the same envelope, so this reads exactly what the commit
/// would have done.
fn would_remove(
    root: &Path,
    kind: &str,
    name: &str,
    preserve_storage: bool,
) -> Result<BTreeSet<String>, String> {
    let mut args = vec!["destroy", kind, name];
    if preserve_storage {
        args.extend(["--storage", "preserve"]);
    }
    args.extend(["--pretend", "--force", "--output", "json"]);
    let output = Command::new(common::bin())
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("{stdout}{stderr}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("{error}: {stdout}"))?;
    Ok(value
        .pointer("/report/data/operations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|operation| operation["kind"] == "delete")
        .filter_map(|operation| operation["path"]["project"].as_str())
        .map(|path| path.replace('\\', "/"))
        .collect())
}

/// Apply the destroy whose preview was just checked, so later iterations see
/// the same dependency graph a user gets when retiring dependants first.
fn remove_recorded(
    root: &Path,
    kind: &str,
    name: &str,
    preserve_storage: bool,
) -> Result<(), String> {
    let mut args = vec!["destroy", kind, name];
    if preserve_storage {
        args.extend(["--storage", "preserve"]);
    }
    args.extend(["--force", "--output", "json"]);
    let output = Command::new(common::bin())
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// The same question, asked of a project that has **no** record of what jails
/// wrote -- which is every project generated before `.jails/` existed, and
/// every project whose `.jails/` somebody deleted.
///
/// The answer changed with the dispatch flip, and the change is the subject.
/// V1 recomputed the paths: it offered each generator a short list of argument
/// shapes and kept the paths of the first that accepted, which meant a kind
/// whose paths depend on an argument nobody could guess went silent -- and a
/// silent `destroy` prints "nothing to destroy" over files that are right
/// there. V2 does not guess at all. `destroy` retires a **recorded** entity,
/// so with no record every kind refuses, and the refusal names the command
/// that would have recorded it.
///
/// That is strictly the better failure, and this test is what says so: not one
/// kind may quietly succeed at deleting files it cannot know it wrote, and not
/// one may refuse without saying what would have made it possible.
/// One scenario's worth of the check below, so the table can be scheduled.
///
/// Every cell is its own temporary directory and its own `jails` processes,
/// which is what makes the table parallelisable at all -- and what made
/// running it on one thread inside one `#[test]` cost eighteen seconds of a
/// four-core machine's time. Findings are returned rather than asserted so
/// the report the caller builds stays in table order however the cells ran.
fn refusals_without_a_record(scenario: &scenarios::Scenario) -> (Vec<String>, usize) {
    let mut findings: Vec<String> = Vec::new();
    let mut refusals = 0usize;
    {
        let root = scenarios::prepare(scenario);
        for step in scenario.steps {
            scenarios::run_step(&root, scenario.name, step);
        }

        // Erase the ledger: what is left is a project jails has no record of.
        let _ = std::fs::remove_dir_all(root.join(".jails"));

        for step in scenario.steps {
            if !matches!(step.first(), Some(&"g") | Some(&"generate")) {
                continue;
            }
            let (kind, name) = (step[1], step[2]);
            if FORWARD_ONLY.contains(&kind) {
                continue;
            }
            let where_ = format!("{}/{kind} {name} (no record)", scenario.name);
            match would_remove(&root, kind, name, false) {
                Ok(removed) => findings.push(format!(
                    "{where_}: destroy succeeded with no record, naming {} path(s). \
                     Without a record jails cannot know it wrote them.",
                    removed.len()
                )),
                Err(message) => {
                    refusals += 1;
                    if !message.contains("fix:") {
                        findings.push(format!(
                            "{where_}: refused without naming what would have recorded \
                             it:\n    {message}"
                        ));
                    }
                }
            }
        }
    }

    (findings, refusals)
}

#[test]
fn destroy_refuses_rather_than_guessing_on_a_project_with_no_record() {
    let per_scenario = common::parallel::map_recording(
        "agreement-no-record",
        SCENARIOS,
        |scenario| scenario.name.to_string(),
        refusals_without_a_record,
    );
    let findings: Vec<String> = per_scenario
        .iter()
        .flat_map(|(findings, _)| findings.clone())
        .collect();
    let refusals: usize = per_scenario.iter().map(|(_, refusals)| refusals).sum();

    assert!(
        findings.is_empty(),
        "destroy on a project with no record is wrong in {} place(s):\n  {}",
        findings.len(),
        findings.join("\n  ")
    );
    // A floor rather than an exact count: the scenario table grows, and a
    // test that pinned the number would fail on every new kind. Zero is the
    // failure worth catching -- it would mean the loop stopped asking.
    assert!(
        refusals > 20,
        "only {refusals} intent(s) were asked; the loop is not covering the scenarios"
    );
}

/// One scenario's worth of the agreement check. See
/// [`refusals_without_a_record`] for why the table is shaped this way.
fn agreement_for(scenario: &scenarios::Scenario) -> Vec<String> {
    let mut findings: Vec<String> = Vec::new();
    {
        let root = scenarios::prepare(scenario);
        // What each step added, so a capability's files are never charged to
        // a generator, and a generator's files are attributed to the command
        // that actually wrote them.
        let mut created_by: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        let mut before = scenarios::file_set(&root);
        for (index, step) in scenario.steps.iter().enumerate() {
            scenarios::run_step(&root, scenario.name, step);
            let after = scenarios::file_set(&root);
            created_by.insert(index, after.difference(&before).cloned().collect());
            before = after;
        }
        let all_created: BTreeSet<String> = created_by.values().flatten().cloned().collect();

        // Scenarios declare prerequisites before their consumers. Retire in
        // reverse so the agreement check does not ask destroy to violate the
        // same reference graph it is meant to protect.
        for (index, step) in scenario.steps.iter().enumerate().rev() {
            if !matches!(step.first(), Some(&"g") | Some(&"generate")) {
                continue;
            }
            let (kind, name) = (step[1], step[2]);
            let created = &created_by[&index];
            let where_ = format!("{}/{kind} {name}", scenario.name);

            let table_backed = kind == "scaffold"
                && created
                    .iter()
                    .any(|path| path.starts_with("src/main/resources/db/migration/"));
            let removed = match would_remove(&root, kind, name, table_backed) {
                Ok(paths) => {
                    if FORWARD_ONLY.contains(&kind) {
                        findings.push(format!(
                            "{where_}: destroy succeeded, but `{kind}` is documented as \
                             forward-only -- one of the two is now wrong"
                        ));
                    }
                    paths
                }
                Err(message) => {
                    if FORWARD_ONLY.contains(&kind) {
                        // The refusal has to *say why*, not merely fail. Which
                        // words it uses is the message's business -- pinning
                        // the exact sentence here is how wording ossifies --
                        // so what is checked is that it names the kind and
                        // hands over something to do instead.
                        assert!(
                            message.contains(kind) && message.contains("fix:")
                                || message.contains("forward-only"),
                            "{where_}: destroy refused without explaining:\n{message}"
                        );
                        continue;
                    }
                    if message.contains("would leave")
                        && message.contains("pointing at nothing")
                        && message.contains("fix:")
                    {
                        // Some forward-only declarations (notably an
                        // association migration) deliberately remain in the
                        // ledger and therefore continue to protect what they
                        // reference. Reference safety is covered directly by
                        // the destroy tests; this test is about path-set
                        // agreement, which cannot be observed for a refused
                        // operation.
                        continue;
                    }
                    findings.push(format!(
                        "{where_}: destroy --pretend failed:\n    {message}"
                    ));
                    continue;
                }
            };

            for path in &removed {
                if !all_created.contains(path) {
                    findings.push(format!(
                        "{where_}: destroy would remove `{path}`, which no jails command in \
                         this scenario created -- the path list and the generator disagree"
                    ));
                }
            }
            for path in created.difference(&removed) {
                if explanation(kind, path).is_none() {
                    findings.push(format!(
                        "{where_}: generate wrote `{path}` and destroy would strand it"
                    ));
                }
            }
            if let Err(message) = remove_recorded(&root, kind, name, table_backed) {
                findings.push(format!(
                    "{where_}: destroy applied differently from its preview:\n    {message}"
                ));
            }
        }

        std::fs::remove_dir_all(&root).ok();
    }

    findings
}

#[test]
fn destroy_removes_exactly_what_generate_created() {
    let findings: Vec<String> = common::parallel::map_recording(
        "agreement-paths",
        SCENARIOS,
        |scenario| scenario.name.to_string(),
        agreement_for,
    )
    .into_iter()
    .flatten()
    .collect();

    assert!(
        findings.is_empty(),
        "generate and destroy disagree in {} place(s):\n  {}\n\n\
         Either add the path to `destroy`'s arm in src/generate.rs, or -- if the file is \
         deliberately kept -- add the reason to ALLOWED_LEFTOVER in this file.",
        findings.len(),
        findings.join("\n  ")
    );
}
