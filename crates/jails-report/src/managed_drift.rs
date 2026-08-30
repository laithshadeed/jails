//! Has anything jails wrote been changed since it wrote it?
//!
//! Compared against the ledger's `current` image — the exact file state the
//! last commit accepted, reader edits included — rather than against a
//! freshly rendered artifact. The distinction is the whole module: a merge
//! deliberately preserves hand edits, so re-rendering and diffing would report
//! every preserved edit as drift, every run, forever.
//!
//! Read-only, like everything in this crate. It reports what moved; deciding
//! what to do about it is `sync`'s.

use crate::diagnostic::{Check, Status};
use crate::model::Project;
use jails_protocol::entity::EntityId;
use jails_protocol::resource::ResourceOwner;
use jails_state::compat::MachineState;

/// Compare every recorded managed output with the live project tree.
///
/// The ledger's `current` image is the exact file state accepted by the last
/// commit, including reader edits preserved by a merge. Comparing with that
/// image detects changes made afterwards without mistaking an earlier,
/// deliberately preserved edit for drift.
pub(crate) fn managed_output_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    // An interrupted transaction explains every difference below, and the two
    // explanations are not interchangeable. A write that stopped part-way --
    // an unwritable directory, a full disk, an IDE lock -- leaves jails' own
    // newer bytes on disk with the ledger still at the older state, which
    // reads exactly like a developer editing five generated files at once.
    // Reporting it that way sent people to `resource repair --strategy
    // roll-forward`, which adopts the half-applied state as the recorded
    // truth: `doctor` goes green over a project whose every insert names a
    // column no migration created.
    //
    // The transaction is still on disk and the next mutating command finishes
    // it under the lock, so what this needs to say is "run it again", once,
    // instead of five accusations.
    if let Some(pending) = interrupted(project) {
        return vec![pending];
    }
    let mut checks = Vec::new();
    for output in &store.outputs {
        let Some((title, fix)) = owner_action(&output.contributors) else {
            continue;
        };
        let path = project.root().join(output.path.as_str());
        match std::fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => checks.push(
                Check::new(
                    Status::Fail,
                    title,
                    format!("recorded output `{}` is missing", output.path),
                )
                .fix(fix),
            ),
            Err(error) => checks.push(
                Check::new(
                    Status::Fail,
                    title,
                    format!("recorded output `{}` cannot be read: {error}", output.path),
                )
                .fix(fix),
            ),
            Ok(bytes)
                if jails_protocol::identity::ObjectId::from_bytes(
                    jails_support::codec::sha256(&bytes),
                ) != output.current.sha256 =>
            {
                checks.push(
                    Check::new(
                        Status::Warn,
                        title,
                        format!(
                            "recorded output `{}` changed since the last jails commit",
                            output.path
                        ),
                    )
                    .fix(fix),
                );
            }
            Ok(_) => {}
        }
    }
    checks
}

/// The one check to make when a transaction did not finish.
///
/// Read-only: it reads the journal, and never recovers anything. Recovery
/// belongs to the commands that take the project lock.
pub(crate) fn interrupted(project: &Project) -> Option<Check> {
    let pending = jails_commit::store::Store::at(project.root()).unfinished_transactions();
    let journal = pending.first()?;
    Some(
        Check::new(
            Status::Fail,
            "transaction",
            format!(
                "transaction {} started and did not finish, so some of jails' own output is \
                 newer than what jails has recorded",
                journal.transaction
            ),
        )
        .fix(
            "run the same command again -- it finishes the interrupted transaction before doing \
             anything new. Do not run `jails resource repair`: it would adopt the half-applied \
             state as the recorded truth",
        ),
    )
}

/// Every migration a resource lifecycle has sealed, against the file on disk.
///
/// A different question from the one above, and the one that had no asker. A
/// migration written by `jails resource field` is not recorded as *managed
/// output* -- it carries no renderer stamp -- so deleting
/// `V002__rename_borower_to_borrower.sql` left `doctor` reporting `25 checks,
/// all clear` while deleting the neighbouring create migration, written by
/// `g scaffold`, was correctly caught. The command's own migration was outside
/// the check that existed.
///
/// The seal is a better authority than the output row anyway: it is the exact
/// content digest of published append-only schema history, which is the thing
/// that must not move.
///
/// The two failures need different advice, and conflating them is what closed
/// a loop once. A **missing** file is restorable from its recorded object. An
/// **edited** one is a deliberate correction the reader made, and
/// `resource repair` would silently overwrite it -- so this says what changed
/// and leaves the choice with the person who made it.
pub(crate) fn migration_seal_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    let mut checks = Vec::new();
    for lifecycle in &store.lifecycles {
        let EntityId::Intent(id) = &lifecycle.entity else {
            continue;
        };
        let title = format!("migrations {}", id.name);
        for seal in &lifecycle.migrations {
            let path = project.root().join(seal.path.as_str());
            match std::fs::read(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => checks.push(
                    Check::new(
                        Status::Fail,
                        title.clone(),
                        format!("sealed migration `{}` is missing", seal.path),
                    )
                    .fix(format!(
                        "jails resource repair {} --strategy roll-forward",
                        id.name
                    )),
                ),
                Err(error) => checks.push(
                    Check::new(
                        Status::Fail,
                        title.clone(),
                        format!("sealed migration `{}` cannot be read: {error}", seal.path),
                    )
                    .fix("make the file readable and run `jails doctor` again"),
                ),
                Ok(bytes)
                    if jails_protocol::identity::ObjectId::from_bytes(
                        jails_support::codec::sha256(&bytes),
                    ) != seal.content_digest =>
                {
                    checks.push(
                        Check::new(
                            Status::Fail,
                            title.clone(),
                            format!(
                                "sealed migration `{}` differs from the bytes jails published",
                                seal.path
                            ),
                        )
                        .fix(concat!(
                            "published schema history is append-only, so restore its exact bytes ",
                            "and append a later migration for the change you want. `jails ",
                            "resource repair --strategy roll-forward` restores them and will ",
                            "discard the edit"
                        )),
                    );
                }
                Ok(_) => {}
            }
        }
    }
    checks
}

fn owner_action(owners: &std::collections::BTreeSet<ResourceOwner>) -> Option<(String, String)> {
    for owner in owners {
        match owner {
            ResourceOwner::Entity(EntityId::Intent(id)) => {
                return Some((
                    format!("managed {}", id.name),
                    format!("jails resource repair {} --strategy roll-forward", id.name),
                ));
            }
            ResourceOwner::Entity(EntityId::Capability(id)) => {
                return Some((
                    format!("capability {}", id.kind.label()),
                    "jails sync".to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}

/// Generated test files that are `@Disabled`, and therefore prove nothing.
///
/// modern.md §13.8. A generator that cannot write a meaningful assertion --
/// a strategy implementation is `return Optional.empty()` with a TODO, and
/// asserting that an accessor returns what was passed in only tests that javac
/// generated the accessor -- writes an honest `@Disabled` naming what to
/// prove. Writing it is right; saying nothing about it afterwards is not. One
/// real project shipped **five of its nine tests disabled**, including both
/// controller tests, and reported green. `CLAUDE.md` already names this
/// failure mode for skipped tier-3 tests; a generated `@Disabled` is the same
/// thing one level down, and the count hides both.
///
/// A `warn`, never a `FAIL`: the file is exactly what jails meant to write,
/// and the work it names is the reader's. What `doctor` owes them is that the
/// number is visible rather than folded into a green tick -- and it keeps
/// answering, which a line in one command's summary does not.
///
/// Only *recorded* output is examined. A hand-written `@Disabled` is a
/// deliberate decision by somebody who can see it in their own diff.
pub(crate) fn disabled_generated_tests(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    let mut pending: Vec<&str> = Vec::new();
    for output in &store.outputs {
        let path = output.path.as_str();
        if !path.starts_with("src/test/java/") || !path.ends_with(".java") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(project.root().join(path)) else {
            continue;
        };
        // Through `blanked`, so an `@Disabled` inside a Javadoc example -- the
        // way `is_spring_boot_test` reads a `@SpringBootTest` -- is not counted
        // as one on a method.
        if jails_java::java::blanked(&source).contains("@Disabled") {
            pending.push(path);
        }
    }
    if pending.is_empty() {
        return Vec::new();
    }
    pending.sort_unstable();
    let named = pending.join("`, `");
    vec![
        Check::new(
            Status::Warn,
            "generated tests",
            format!(
                "{} generated test file(s) are @Disabled, so `mvn test` reports green over \
                 them: `{named}`",
                pending.len()
            ),
        )
        .fix(
            "each names what to prove in its @Disabled reason. Write the class it covers, \
             then delete the annotation -- or delete the test, which is the honest answer \
             when the assertion was never going to be worth making",
        ),
    ]
}

/// Migrations jails wrote and nobody filled in.
///
/// modern.md §13.7. `jails g migration add_customer_id_index` writes one line
/// -- `-- Forward-only migration. Write explicit SQL below.` -- and that is a
/// correct thing to write: jails cannot know the SQL, and the value of the
/// command is a correctly numbered file at the right path. What is not correct
/// is what happens next. Flyway applies the file, records its checksum, and
/// never mentions it again, so the migration history asserts that
/// `messages.customer_id` was indexed and the column has no index. A blank
/// migration is an unusual thing to *want*; leaving it silent is the defect.
///
/// A `warn`, because the file is the reader's to fill in, and named
/// individually because the whole point is that the history claims something
/// each one does not do.
pub(crate) fn empty_migration_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    let mut blank: Vec<&str> = Vec::new();
    for output in &store.outputs {
        let path = output.path.as_str();
        if !path.starts_with("src/main/resources/db/migration/") || !path.ends_with(".sql") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(project.root().join(path)) else {
            continue;
        };
        let statements = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        if statements.trim().is_empty() {
            blank.push(path);
        }
    }
    if blank.is_empty() {
        return Vec::new();
    }
    blank.sort_unstable();
    vec![
        Check::new(
            Status::Warn,
            "migrations",
            format!(
                "{} migration(s) contain no SQL, so the schema history records a change \
                 that did not happen: `{}`",
                blank.len(),
                blank.join("`, `")
            ),
        )
        .fix(
            "write the statement each one is named for. A migration that is applied empty \
             is sealed empty -- the history says it ran and the database does not have it, \
             and the correction has to be a later migration",
        ),
    ]
}
