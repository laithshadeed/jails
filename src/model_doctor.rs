//! The `doctor` checks over a canonical project's managed tree.
//!
//! An edited or deleted managed file must not be invisible
//! to a report that ends `all clear`: silence about a question nobody can see
//! you failed to ask is the worst answer available.
//!
//! Three questions, each answered from the lock rather than from a fresh
//! render, for `managed_drift`'s reason: a merge deliberately preserves reader
//! edits, so re-rendering and diffing reports every preserved edit as drift,
//! every run, forever.
//!
//! - **Has a managed file been deleted?** `Fail`. `jails sync` refuses while
//!   one is gone -- "restore it or eject its implementation boundary; nothing
//!   was written" -- so the project cannot converge until somebody acts.
//! - **Has one been changed?** `Warn`, and deliberately not a fault: the merge
//!   preserves the edit and carries it forward.
//! - **Does the model declare something the lock has not accepted?** `Warn`.
//!   That is the ordinary state between `g` and `sync`.
//!
//! The two `fix:` lines are what the binary actually does, not what the design
//! suggests. `jails sync` clears the third and neither of the first two: on an
//! edited file it writes nothing and the warning stands, which is the point of
//! the merge, and on a deleted one it refuses.

use jails_model::StableId;
use jails_report::doctor::{Check, Status};
use jails_support::Result;

/// Every canonical check, or none when this project is not canonical.
///
/// Capture failure is reported as a check rather than raised: `doctor` is the
/// command a reader runs when something is already wrong, and it must not be
/// the second thing that fails.
pub(crate) fn checks() -> Vec<Check> {
    if !crate::model_command::project_root().is_some_and(|root| crate::model_command::owns(&root)) {
        return Vec::new();
    }
    match collect() {
        Ok(checks) => checks,
        Err(error) => vec![
            Check::new(
                Status::Skip,
                "canonical model",
                format!("could not read the model: {error}"),
            )
            .fix("jails model check"),
        ],
    }
}

fn collect() -> Result<Vec<Check>> {
    let manifest = crate::model_command::resolve_manifest(None)?;
    let root = crate::model_command::root()?;
    let (source, model) = crate::model_command::load_model(&root, &manifest, crate::Output::Human)?;
    let snapshot = jails_project::capture::capture(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        None,
        &[],
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| jails_support::Failure::Told(error.to_string()))?;

    let mut checks = Vec::new();
    let mut edited = Vec::new();
    let mut missing = Vec::new();
    let mut eject_target = None;
    if let Some(projection) = snapshot.accepted_projection.as_ref() {
        for (path, file) in &projection.files {
            match snapshot.files.get(path) {
                None => missing.push(path.as_str().to_string()),
                Some(captured) if captured.bytes != file.bytes => {
                    edited.push(path.as_str().to_string());
                    // Only an *ejectable* artifact may be offered as one.
                    // `jails model eject art_ent_order_record` refuses -- "records
                    // and ports remain managed ABI" -- so naming it would be a
                    // `fix:` line that cannot be followed.
                    if file.provenance.ejectable {
                        eject_target
                            .get_or_insert_with(|| file.provenance.ejection_target().to_string());
                    }
                }
                Some(_) => {}
            }
        }
    }

    // A deleted managed file is a `Fail` because `sync` refuses while it is
    // gone -- "restore it or eject its implementation boundary; nothing was
    // written" -- so the project cannot converge until somebody acts.
    checks.push(match missing.is_empty() {
        true => Check::new(
            Status::Ok,
            "managed output",
            "every file the lock accepted is on disk",
        ),
        false => Check::new(
            Status::Fail,
            "managed output",
            format!(
                "{} deleted; `jails sync` refuses while it is gone",
                list(&missing)
            ),
        )
        .fix("restore the file from version control"),
    });

    // An *edited* one is not a fault at all: the merge preserves it and the
    // next model change carries it forward. It is a `Warn` for
    // `template_override_checks`' reason -- a supported thing to be doing,
    // and the reader is entitled to know they are doing it.
    checks.push(match edited.is_empty() {
        true => Check::new(
            Status::Ok,
            "managed edits",
            "no generated file has been changed since the lock accepted it",
        ),
        false => {
            let check = Check::new(
                Status::Warn,
                "managed edits",
                format!(
                    "{} changed since generation; jails merges the edit forward on every sync",
                    list(&edited)
                ),
            );
            match eject_target {
                Some(id) => check.fix(format!(
                    "nothing, or `jails model eject {id}` to own the file outright"
                )),
                // Managed ABI. There is nothing to run, and saying so beats
                // naming a command that would refuse.
                None => check,
            }
        }
    });

    // The lock's accepted model is what the last executed plan agreed to.
    // Declared-and-not-accepted is the ordinary state between `g` and `sync`.
    // Capabilities as well as entities: a canonical project declares
    // `cap json` in the model, and `doctor`'s `jails.toml` check reported
    // "records none -- nothing to reconcile" about it. That row now says where
    // they live; this one is what actually reconciles them.
    let declared = &snapshot.model.model;
    let mut pending = Vec::new();
    match snapshot.accepted_model.as_ref() {
        None => {
            pending.extend(
                declared
                    .entities
                    .values()
                    .map(|entity| entity.label.clone()),
            );
            pending.extend(
                declared
                    .capabilities
                    .keys()
                    .map(|id| id.as_str().to_string()),
            );
        }
        Some(accepted) => {
            pending.extend(
                declared
                    .entities
                    .iter()
                    .filter(|(id, entity)| accepted.entities.get(*id) != Some(*entity))
                    .map(|(_, entity)| entity.label.clone()),
            );
            pending.extend(
                declared
                    .capabilities
                    .keys()
                    .filter(|id| !accepted.capabilities.contains_key(*id))
                    .map(|id| id.as_str().to_string()),
            );
        }
    }
    pending.sort();
    checks.push(match pending.is_empty() {
        true => Check::new(
            Status::Ok,
            "model accepted",
            "the lock has accepted everything the model declares",
        ),
        false => Check::new(
            Status::Warn,
            "model accepted",
            format!("{} not in the accepted model", list(&pending)),
        )
        .fix("jails sync"),
    });

    checks.push(published_history(&root, &snapshot));
    checks.push(unwritten_migrations(&snapshot));
    checks.push(disabled_tests(&snapshot));
    checks.extend(schema_lineage(&snapshot));
    Ok(checks)
}

/// Generated tests that are present and will not run.
///
/// A test that does not run is worse than no test: the build is green either
/// way and only one of the two says so. jails disables a companion it cannot
/// honestly drive rather than guessing a value that would not compile, and the
/// plan names each one as it is written -- this is the same fact afterwards,
/// for a reader who did not watch the plan go by.
fn disabled_tests(snapshot: &jails_contracts::WorkspaceSnapshot) -> Check {
    let Some(projection) = snapshot.accepted_projection.as_ref() else {
        return Check::new(
            Status::Skip,
            "generated tests",
            "nothing has been accepted into the managed tree yet",
        );
    };
    let mut disabled = projection
        .files
        .iter()
        .filter(|(path, file)| {
            path.as_str().ends_with(".java")
                && std::str::from_utf8(&file.bytes).is_ok_and(|source| source.contains("@Disabled"))
        })
        .map(|(path, _)| path.as_str().to_string())
        .collect::<Vec<_>>();
    if disabled.is_empty() {
        return Check::new(Status::Ok, "generated tests", "every generated test runs");
    }
    disabled.sort();
    Check::new(
        Status::Warn,
        "generated tests",
        format!(
            "{} disabled -- jails had no sample it could honestly send",
            list(&disabled)
        ),
    )
    .fix("write the case by hand, or declare a type jails can sample")
}

/// A migration file that carries no SQL at all.
///
/// `g migration` writes a correctly numbered file at the right path and a
/// comment, because jails cannot know the SQL and a wrong guess is worse than
/// none. Leaving it *silent* is the defect: Flyway applies it, checksums it,
/// and never mentions it again -- so the history asserts a change that did not
/// happen. A warning rather than a fault, because it is the reader's file and
/// an unfinished one is an ordinary state to be in mid-change.
fn unwritten_migrations(snapshot: &jails_contracts::WorkspaceSnapshot) -> Check {
    let mut empty = Vec::new();
    for record in &snapshot.migration_history.records {
        let Some(file) = snapshot.files.get(&record.path) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        if text
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with("--"))
        {
            empty.push(record.path.as_str().to_string());
        }
    }
    if empty.is_empty() {
        return Check::new(
            Status::Ok,
            "migration bodies",
            "every migration in the history says something",
        );
    }
    empty.sort();
    Check::new(
        Status::Warn,
        "migration bodies",
        format!(
            "{} contain no SQL, so Flyway will apply and checksum a change that did not happen",
            list(&empty)
        ),
    )
    .fix("write the statements, or delete the file if it is not needed yet")
}

/// Is the schema history jails published still the history it published?
///
/// **Append-only is the whole of it.** A migration already applied to a
/// database cannot be rewritten: Flyway refuses on the checksum, and a
/// database that ran the old text is not described by the new one. So unlike a
/// generated Java file -- where an edit is a supported thing to be doing and
/// the merge carries it forward -- an edited or deleted migration is a fault,
/// and this is the only check that can see it. The captured
/// `migration_history` is read fresh from the tree on every capture, so it
/// agrees with whatever the file says now; only the lock's seal remembers what
/// was published.
fn published_history(
    root: &std::path::Path,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Check {
    let mut faults = Vec::new();
    for (path, sealed) in &snapshot.accepted_migrations {
        match std::fs::read(root.join(path.as_str())) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                faults.push(format!("`{path}` is missing"));
            }
            Err(error) => faults.push(format!("`{path}` cannot be read: {error}")),
            Ok(bytes) => {
                if jails_workspace::digest(&bytes).as_ref() != Ok(sealed) {
                    faults.push(format!("`{path}` differs from the bytes jails published"));
                }
            }
        }
    }
    if faults.is_empty() {
        return Check::new(
            Status::Ok,
            "sealed migrations",
            match snapshot.accepted_migrations.len() {
                0 => "no schema history has been published yet".to_string(),
                n => format!("all {n} published migration(s) are byte-for-byte as written"),
            },
        );
    }
    faults.sort();
    Check::new(
        Status::Fail,
        "sealed migrations",
        format!("{}; schema history is append-only", faults.join("; ")),
    )
    .fix("restore the file from version control, then write a new forward migration for the change")
}

/// Does every column a stored entity's record carries exist in the schema the
/// lock accepted?
///
/// "Are these the bytes jails wrote" is a different question from "is this
/// project coherent": a torn transaction or a half-carried rename leaves the
/// Java carrying a component the accepted schema does not, with every file
/// byte-identical to what jails wrote, and only a query at runtime would
/// find it.
///
/// **Both sides are the compiler's own answer.** `storage_columns` says what
/// columns the storage lowering emits for an entity, and the check asks it
/// twice -- once of the declared model, once of the same entity by stable ID
/// in the accepted model. Reading the columns back out of migration text
/// would be a second description of a decision the compiler already made, and
/// the two would drift with nothing to say which was right.
///
/// **Unknown widens.** No accepted model, or an entity the accepted model
/// does not have, produces no check rather than an accusation: an entity
/// nothing has accepted yet is what `model accepted` above already reports.
fn schema_lineage(snapshot: &jails_contracts::WorkspaceSnapshot) -> Vec<Check> {
    let mut checks = Vec::new();
    let Some(accepted) = snapshot.accepted_model.as_ref() else {
        return checks;
    };
    for entity in snapshot.model.model.entities.values() {
        if !entity.active || !entity.facets.contains(&jails_model::Facet::Repository) {
            continue;
        }
        let Some(before) = accepted.entities.get(&entity.id) else {
            continue;
        };
        let declared = jails_compiler::storage_columns(accepted, before)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let table = entity.names.sql_table.as_str();
        let carried = jails_compiler::storage_columns(&snapshot.model.model, entity);
        let missing = carried
            .iter()
            .filter(|column| !declared.contains(*column))
            .map(String::as_str)
            .collect::<Vec<_>>();
        let title = format!("schema {}", entity.names.java_type);
        checks.push(match missing.is_empty() {
            true => Check::new(
                Status::Ok,
                title,
                format!("`{table}` has every column the record carries"),
            ),
            false => Check::new(
                Status::Fail,
                title,
                format!(
                    "`{table}` is missing {}, which `{}` carries",
                    missing.join(", "),
                    entity.names.java_type
                ),
            )
            .fix("jails sync"),
        });
    }
    checks
}

/// Name up to three paths, then say how many more there are.
///
/// A project whose whole generated tree was deleted would otherwise print
/// every path, and a check nobody reads to the end reports nothing.
fn list(paths: &[String]) -> String {
    let shown = paths
        .iter()
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match paths.len() {
        0..=3 => shown,
        n => format!("{shown} and {} more", n - 3),
    }
}
