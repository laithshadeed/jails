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
    // **A stdin that cannot be read has not answered.** Defaulting to yes
    // there is how a pipeline deletes something nobody saw; defaulting to a
    // silent *no* is nearly as bad, because the command then exits 0 having
    // done nothing and the script goes on believing it worked. So an
    // unanswerable prompt is a refusal with an exit status, and `--force` is
    // how a script says yes in advance.
    let asked = std::io::stdin().lock().read_line(&mut answer);
    if matches!(&asked, Ok(0)) || asked.is_err() {
        return Some(Err(Failure::Told(
            "this deletion needs an answer and nothing is connected to read one from.\n       fix: rerun with `--force` to confirm in advance".to_string(),
        )));
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
        // **Said out loud, because the lines above look exactly like a report
        // of what happened.** `create .jails/generated/...` reads the same
        // whether it is a preview or a receipt, and the one place a reader can
        // tell them apart should not be the flag they typed a moment ago.
        if invocation.pretend {
            println!("nothing was written.");
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

/// Reader sources that name a managed type this transition removes.
///
/// **jails does not delete a file it does not own**, and the engine it
/// replaces did: `destroy strategy` swept every main source directory for
/// implementations, on the grounds that one left behind stops the project
/// compiling. That is true and it is not jails' file. The reader is told
/// instead, by name, at the moment it becomes their problem -- which is the
/// half the sweep was actually for.
///
/// Matched on the whole identifier, the same rule `rename` follows, and read
/// out of the captured reader tree rather than off disk so it agrees with the
/// snapshot every other decision here was made from.
pub(crate) fn stranded_reader_references(
    root: &std::path::Path,
    current_model: &jails_model::AppModel,
    next_model: &jails_model::AppModel,
) -> Vec<String> {
    let surviving: std::collections::BTreeSet<&str> = declared_types(next_model).collect();
    let removed: Vec<&str> = declared_types(current_model)
        .filter(|name| !surviving.contains(name))
        .collect();
    if removed.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    // Read from the tree rather than from the capture: which reader
    // directories a plan captures follows from what it needs to *write*, and
    // this needs to look everywhere the reader keeps Java.
    for tree in ["src/main/java", "src/test/java"] {
        for path in java_sources(&root.join(tree)) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let blanked = jails_java::java::blanked(&source);
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for name in &removed {
                if !names_identifier(&blanked, name) {
                    continue;
                }
                lines.push(format!(
                    "  note    {relative} still names `{name}`, which this removes -- it is your file, so jails left it alone"
                ));
            }
        }
    }
    lines
}

/// Every `.java` file under a directory, deepest order unimportant.
fn java_sources(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "java")
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every Java type name this model declares.
fn declared_types(model: &jails_model::AppModel) -> impl Iterator<Item = &str> {
    model
        .entities
        .values()
        .map(|entity| entity.names.java_type.as_str())
        .chain(
            model
                .components
                .values()
                .map(|component| component.name.as_str()),
        )
        .chain(
            model
                .operations
                .values()
                .map(|operation| operation.names.java_type.as_str()),
        )
        .chain(model.units.values().map(|unit| unit.java_type.as_str()))
}

/// Whether this source names the identifier, rather than merely containing its
/// letters: `RewardRule` is in `HandWrittenRewardRule` and means nothing there.
fn names_identifier(source: &str, name: &str) -> bool {
    let boundary = |character: Option<char>| {
        character.is_none_or(|character| !character.is_alphanumeric() && character != '_')
    };
    source.match_indices(name).any(|(index, _)| {
        boundary(source[..index].chars().next_back())
            && boundary(source[index + name.len()..].chars().next())
    })
}
