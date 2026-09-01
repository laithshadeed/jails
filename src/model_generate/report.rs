//! What the reader is told about a plan, and the one question they are asked.
//!
//! **Split out by audience.** Everything here is about the transition as the
//! reader sees it -- the files it would touch, the tests it writes that will
//! not run, the deletions it wants permission for, the bundle it can export
//! and review elsewhere. None of it decides what the project becomes.
//!
//! The refusal is here rather than beside the apply path for the same reason:
//! it is a question put to a person, and what it asks with is the same preview
//! `--pretend` prints.

use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use std::path::Path;

/// Put a deletion to the reader before it happens.
///
/// **"It exists" is not ownership.** `remove` and `destroy` delete every
/// generated file the plan names, and a `CsvReader` somebody spent an
/// afternoon on looks exactly like the stub jails wrote. Refusing would make
/// them unusable on the projects that got the most out of them; deleting
/// silently is how an afternoon disappears. So the list is shown and the
/// question is asked, and `--force` is the answer given in advance.
///
/// `None` means nothing is in the way. `Some` carries the whole outcome,
/// including the successful "aborted" one -- a reader who says no got what
/// they asked for.
pub(super) fn refuse_unconfirmed_deletions(
    bundle: &jails_contracts::PlanBundle,
    invocation: &Invocation,
) -> Option<Result<()>> {
    use std::io::BufRead as _;
    // **Only the commands whose purpose is deletion.** A `g field` that
    // supersedes a companion, or a `sync` converging a tree, deletes files as
    // a consequence of what the model now says -- asking there would put a
    // prompt in front of every ordinary mutation. `remove` and `destroy` are
    // the two where deletion *is* the request, and the two where a reader's
    // afternoon of edits can be in the files named.
    let removal = invocation
        .command_path
        .first()
        .is_some_and(|command| command == "remove" || command == "destroy");
    if !removal || invocation.force || invocation.output != Output::Human {
        return None;
    }
    let deletions = crate::model_command::preview_lines(bundle)
        .into_iter()
        .filter(|line| line.trim_start().starts_with("delete"))
        .collect::<Vec<_>>();
    if deletions.is_empty() {
        return None;
    }
    println!(
        "This removes {} generated file{}:",
        deletions.len(),
        if deletions.len() == 1 { "" } else { "s" }
    );
    for line in &deletions {
        println!("{line}");
    }
    print!("Delete them? [y/N] ");
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    // A closed stdin is a no: a command that cannot ask has not been answered,
    // and defaulting to yes there is how a pipeline deletes something nobody
    // saw. `--force` is how a script says yes.
    if std::io::stdin().lock().read_line(&mut answer).is_err() {
        answer.clear();
    }
    if answer.trim().eq_ignore_ascii_case("y") {
        return None;
    }
    println!("aborted; nothing was written.");
    Some(Ok(()))
}

/// The tests this plan writes that will not run.
///
/// **A test that does not run is worse than no test**, because the build is
/// green either way and only one of the two says so. jails disables a
/// companion it cannot honestly drive -- a component whose type it has no
/// sample for, a request body it cannot construct -- rather than guessing a
/// value that would not compile or emitting nothing and dropping the coverage
/// silently. Saying which files, at plan time, is what keeps that a decision
/// the reader saw rather than a surprise in the report.
///
/// Read off the rendered bytes rather than from a note beside them, so a
/// renderer that starts or stops disabling something cannot forget to say so.
fn disabled_tests(bundle: &jails_contracts::PlanBundle) -> Vec<String> {
    let mut disabled = bundle
        .trees
        .values()
        .flat_map(|tree| tree.entries.iter())
        .filter(|(path, entry)| {
            path.as_str().ends_with(".java")
                && bundle
                    .blobs
                    .get(&entry.blob)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .is_some_and(|source| source.contains("@Disabled"))
        })
        .map(|(path, _)| path.as_str().to_string())
        .collect::<Vec<_>>();
    disabled.sort();
    disabled.dedup();
    disabled
}

pub(crate) fn report_plan(
    bundle: &jails_contracts::PlanBundle,
    invocation: &Invocation,
) -> Result<()> {
    if invocation.output == Output::Human {
        println!(
            "plan {}: {} operations, {} managed files",
            bundle.plan.digest.as_str(),
            bundle.plan.operations.len(),
            bundle.plan.summary.managed_files
        );
        for line in crate::model_command::preview_lines(bundle) {
            println!("{line}");
        }
        if invocation.ast {
            println!(
                "model patch: {}",
                String::from_utf8_lossy(&bundle.plan.input.bytes)
            );
        }
        if invocation.diff {
            for operation in &bundle.plan.operations {
                println!("  {operation:?}");
            }
        }
        for path in disabled_tests(bundle) {
            println!("  test-disabled  {path}");
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(bundle)
                .map_err(|error| Failure::Told(format!("could not encode exact plan: {error}")))?
        );
    }
    Ok(())
}

pub(crate) fn write_bundle(path: &Path, bundle: &jails_contracts::PlanBundle) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(bundle)
        .map_err(|error| Failure::Told(format!("could not encode exact plan: {error}")))?;
    jails_support::apply::put_outside_project_private_atomic(path, encoded)
}
