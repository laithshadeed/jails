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
        "HttpClientsConfig.java",
        "shared registration: a second client still needs the group",
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

/// Kinds whose `destroy` is a documented refusal rather than a delete.
const FORWARD_ONLY: &[&str] = &["migration", "association"];

fn explanation(kind: &str, rel: &str) -> Option<&'static str> {
    ALLOWED_LEFTOVER
        .iter()
        .find(|(for_kind, matcher, _)| {
            (for_kind.is_empty() || *for_kind == kind) && rel.contains(matcher)
        })
        .map(|(_, _, why)| *why)
}

/// `destroy --pretend` prints one absolute path per line it would remove.
fn would_remove(root: &Path, kind: &str, name: &str) -> Result<BTreeSet<String>, String> {
    let output = Command::new(common::bin())
        .current_dir(root)
        .args(["destroy", kind, name, "--pretend", "--force"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("{stdout}{stderr}"));
    }
    let root = root.canonicalize().unwrap();
    Ok(stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("would remove "))
        .map(|path| {
            let path = Path::new(path.trim());
            let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            path.strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect())
}

#[test]
fn destroy_removes_exactly_what_generate_created() {
    let mut findings: Vec<String> = Vec::new();

    for scenario in SCENARIOS {
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

        for (index, step) in scenario.steps.iter().enumerate() {
            if !matches!(step.first(), Some(&"g") | Some(&"generate")) {
                continue;
            }
            let (kind, name) = (step[1], step[2]);
            let created = &created_by[&index];
            let where_ = format!("{}/{kind} {name}", scenario.name);

            let removed = match would_remove(&root, kind, name) {
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
                        assert!(
                            message.contains("forward-only"),
                            "{where_}: destroy failed with something other than the \
                             forward-only refusal:\n{message}"
                        );
                        continue;
                    }
                    findings.push(format!("{where_}: destroy --pretend failed:\n    {message}"));
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
        }

        std::fs::remove_dir_all(&root).ok();
    }

    assert!(
        findings.is_empty(),
        "generate and destroy disagree in {} place(s):\n  {}\n\n\
         Either add the path to `destroy`'s arm in src/generate.rs, or -- if the file is \
         deliberately kept -- add the reason to ALLOWED_LEFTOVER in this file.",
        findings.len(),
        findings.join("\n  ")
    );
}
