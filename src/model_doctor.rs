//! The `doctor` checks a canonical project can answer and a legacy one cannot.
//!
//! `doctor`'s managed-output checks read `.jails/ledger.toml`. A canonical
//! project has no ledger, so on one of those `jails doctor` reported *nothing*
//! about the tree it generates -- an edited file under `.jails/generated` was
//! invisible, and the report still ended `all clear`. Silence about a question
//! nobody can see you failed to ask is the worst answer available.
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
//!   preserves the edit and carries it forward. Checked by editing a generated
//!   record, adding a field, and confirming the hand-written line survived the
//!   re-render.
//! - **Does the model declare something the lock has not accepted?** `Warn`.
//!   That is the ordinary state between `g` and `sync`.
//!
//! The two `fix:` lines are what the binary actually does, not what the design
//! suggests. `jails sync` clears the third and neither of the first two: on an
//! edited file it writes nothing and the warning stands, which is the point of
//! the merge, and on a deleted one it refuses.

use jails_report::doctor::{Check, Status};
use jails_support::Result;

/// Every canonical check, or none when this project is not canonical.
///
/// Capture failure is reported as a check rather than raised: `doctor` is the
/// command a reader runs when something is already wrong, and it must not be
/// the second thing that fails.
pub(crate) fn checks() -> Vec<Check> {
    if !crate::model_command::owns() {
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
    let (source, model) =
        crate::model_command::load_model_at(&root, &manifest, crate::Output::Human)?;
    let snapshot = jails_workspace::capture(&root, &manifest, source.as_bytes(), model)
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
    // written" -- so the project cannot converge until somebody acts. Measured
    // against the binary, not assumed from the design.
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
    // next model change carries it forward, which this was checked against by
    // adding a field and confirming the hand-written line survived. It is a
    // `Warn` for `template_override_checks`' reason -- a supported thing to be
    // doing, and the reader is entitled to know they are doing it.
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
    let declared = &snapshot.model.model;
    let pending = match snapshot.accepted_model.as_ref() {
        None => (!declared.entities.is_empty()).then_some(declared.entities.len()),
        Some(accepted) => {
            let count = declared
                .entities
                .iter()
                .filter(|(id, entity)| accepted.entities.get(*id) != Some(*entity))
                .count();
            (count > 0).then_some(count)
        }
    };
    checks.push(match pending {
        None => Check::new(
            Status::Ok,
            "model accepted",
            "the lock has accepted every declared entity",
        ),
        Some(count) => Check::new(
            Status::Warn,
            "model accepted",
            format!(
                "{count} declared entit{} not in the accepted model",
                if count == 1 { "y is" } else { "ies are" }
            ),
        )
        .fix("jails sync"),
    });

    Ok(checks)
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
