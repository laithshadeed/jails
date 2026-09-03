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
/// question is asked, and `--yes` is the answer given in advance.
///
/// **The question is asked before the encoding is chosen.** `--output json`
/// used to skip the whole check: `jails --output json destroy scaffold Task`
/// deleted fourteen files and reported `"files_deleted": 14` without asking,
/// because only the human report reached the prompt. Consent is about the
/// files, not about how the answer is printed -- so an encoding that has
/// nobody to ask refuses, in its own envelope, and `--yes` is how a script
/// says yes.
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
    if !removal || invocation.consented {
        return None;
    }
    let deletions = crate::plan_delta::preview_lines(bundle)
        .into_iter()
        .filter(|line| line.trim_start().starts_with("delete"))
        .collect::<Vec<_>>();
    if deletions.is_empty() {
        return None;
    }
    // A machine encoding has no terminal behind it, and the envelope is the
    // whole answer: the refusal names the files it would have deleted and the
    // flag that authorises them.
    if invocation.output != Output::Human {
        return Some(Err(Failure::Told(format!(
            "this deletes {} generated file{} and nothing has consented to it: {}.\n       fix: rerun with `--yes` to confirm in advance",
            deletions.len(),
            if deletions.len() == 1 { "" } else { "s" },
            deletions
                .iter()
                .map(|line| line.split_whitespace().nth(1).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ")
        ))));
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
    // unanswerable prompt is a refusal with an exit status, and `--yes` is
    // how a script says yes in advance.
    let asked = std::io::stdin().lock().read_line(&mut answer);
    if matches!(&asked, Ok(0)) || asked.is_err() {
        return Some(Err(Failure::Told(
            "this deletion needs an answer and nothing is connected to read one from.\n       fix: rerun with `--yes` to confirm in advance".to_string(),
        )));
    }
    if answer.trim().eq_ignore_ascii_case("y") {
        return None;
    }
    println!("aborted; nothing was written.");
    Some(Ok(()))
}

/// What the compiler noticed, as report lines.
///
/// **A note is not a refusal, and must not dress as one.** These were written
/// with the `jails:` prefix every failure wears, on stderr, above the report
/// -- so a shape jails generated on purpose and a command that would not run
/// looked identical, and the two lines the reader saw most often were the two
/// that never meant anything had gone wrong. They are `note` rows in the file
/// list now, beside the `note` a stranded reader reference already prints:
/// same column, same stream, and the report is one thing again.
pub(crate) fn notice_lines(diagnostics: &[jails_contracts::CompilerDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .flat_map(|diagnostic| {
            [
                format!("  note    {}", diagnostic.message),
                format!("          fix: {}", diagnostic.fix),
            ]
        })
        .collect()
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

/// The report as one JSON value: the same status, the same list, the same
/// notes the human output carries.
///
/// **One projection, two encodings.** `--output json` used to print the
/// `Execution` receipt -- four counts and a digest, no list -- so a caller
/// could not learn from JSON what a reader learns from the screen, and the
/// bundle it printed for a preview was a third shape again. The bundle is
/// what `--plan-out` writes, because that is the reviewed transition; this
/// is the report, and `files` is the very list `Delta` renders as lines.
pub(crate) fn json_report(
    status: &str,
    name: &str,
    bundle: &jails_contracts::PlanBundle,
    delta: &crate::plan_delta::Delta,
    notes: &[String],
) -> serde_json::Value {
    // **A report says what happened, and a re-apply's plan is older than the
    // files.** `model apply` of a bundle whose transition is already on disk
    // has before-images from when those paths did not exist, so the plan
    // reads `create` while the executor wrote nothing; the human report says
    // "nothing to do" for exactly this reason, and the encodings must not
    // disagree about it.
    let idle = status == "nothing-to-do";
    let files = match idle {
        true => Vec::new(),
        false => delta
            .changes
            .iter()
            .map(|change| serde_json::json!({"verb": change.verb, "path": change.path}))
            .collect::<Vec<_>>(),
    };
    let unchanged = match idle {
        true => delta.changes.len() + delta.unchanged,
        false => delta.unchanged,
    };
    serde_json::json!({
        "schema": "jails.command-result.v2",
        "status": status,
        "command": name,
        "plan_digest": bundle.plan.digest.as_str(),
        "summary": match idle {
            true => "nothing to do".to_string(),
            false => delta.summary(),
        },
        "unchanged": unchanged,
        "files": files,
        "model": crate::plan_delta::model_hunk(bundle)
            .into_iter()
            .map(|line| line.trim_start().to_string())
            .collect::<Vec<_>>(),
        "notes": notes,
    })
}

pub(crate) fn report_plan(
    bundle: &jails_contracts::PlanBundle,
    invocation: &Invocation,
) -> Result<()> {
    if invocation.output == Output::Human {
        // The same counts the applied report prints, off the same walk: a
        // preview whose summary is shaped differently from the receipt is a
        // second description of one transition.
        let delta = crate::plan_delta::preview(bundle);
        println!(
            "plan {}: {} operations, {}",
            bundle.plan.digest.as_str(),
            bundle.plan.operations.len(),
            delta.summary()
        );
        for line in crate::plan_delta::model_hunk(bundle) {
            println!("{line}");
        }
        for line in &delta.lines() {
            println!("{line}");
        }
        report_review(bundle, invocation);
        for path in disabled_tests(bundle) {
            println!("  test-disabled  {path}");
        }
        // **Said out loud, because the lines above look exactly like a report
        // of what happened.** `create src/main/java/...` reads the same
        // whether it is a preview or a receipt, and the one place a reader can
        // tell them apart should not be the flag they typed a moment ago.
        if invocation.pretend {
            println!("nothing was written.");
        }
    } else {
        let delta = crate::plan_delta::preview(bundle);
        println!(
            "{}",
            serde_json::to_string_pretty(&json_report("planned", "plan", bundle, &delta, &[]))
                .map_err(|error| Failure::Told(format!("could not encode the report: {error}")))?
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
/// **jails does not delete a file it does not own.** A hand-written
/// implementation left behind implementing a deleted interface stops the
/// project compiling, and it is still not jails' file, so the reader is told
/// by name, at the moment it becomes their problem.
///
/// Matched on the whole identifier, the same rule `rename` follows. It reads
/// the tree rather than the capture, and that is deliberate: which reader
/// directories a plan captures follows from what it needs to *write*, while
/// this needs to look everywhere the reader keeps Java. `managed` is what
/// the accepted projection names: those files sit in the same tree and are
/// jails' to change, so they are not the reader's to be told about.
pub(crate) fn stranded_reader_references(
    root: &std::path::Path,
    current_model: &jails_model::AppModel,
    next_model: &jails_model::AppModel,
    managed: &std::collections::BTreeSet<jails_contracts::ProjectPath>,
) -> Vec<String> {
    let surviving: std::collections::BTreeSet<&str> = declared_types(next_model).collect();
    let removed: Vec<&str> = declared_types(current_model)
        .filter(|name| !surviving.contains(name))
        .collect();
    if removed.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for tree in ["src/main/java", "src/test/java"] {
        for path in java_sources(&root.join(tree)) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let blanked = jails_project::java::blanked(&source);
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if jails_contracts::ProjectPath::parse(relative.clone())
                .is_ok_and(|relative| managed.contains(&relative))
            {
                continue;
            }
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
fn java_sources(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
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

/// What `--ast` and `--diff` add, on a preview and on a commit alike.
///
/// **`--ast` is the transition as values and `--diff` is the bytes.**
///
/// Both are printed after a commit too, because "what did that change" is the
/// same question whether it is asked before or after -- and the plan is the
/// same value either way, which is what makes the two answers agree.
pub(crate) fn report_review(bundle: &jails_contracts::PlanBundle, invocation: &Invocation) {
    if invocation.output != Output::Human {
        return;
    }
    if invocation.ast {
        println!(
            "model patch: {}",
            String::from_utf8_lossy(&bundle.plan.input.bytes)
        );
        for operation in &bundle.plan.operations {
            println!("  {operation:?}");
        }
    }
    if invocation.diff {
        for line in unified_diff(bundle) {
            println!("{line}");
        }
    }
}

/// Every managed file this plan changes, as a unified diff.
///
/// **The managed tree is published whole and reviewed per file.**
/// `PublishMergedTree` carries two content digests because the tree is what
/// the executor swaps atomically; a reader reviewing a change wants the lines,
/// so the two trees are walked here and diffed pairwise. Reader-owned files
/// come from their own operations, which carry a before and an after image
/// each.
fn unified_diff(bundle: &jails_contracts::PlanBundle) -> Vec<String> {
    use jails_contracts::PlannedOperation as Op;

    let text = |digest: &jails_contracts::ContentDigest| {
        bundle
            .blobs
            .get(digest)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_string)
    };
    let mut lines = Vec::new();
    let mut show = |verb: &str, path: &str, before: Option<String>, after: Option<String>| {
        if let Some(diff) =
            jails_support::unified::diff(path, before.as_deref(), after.as_deref(), 3)
        {
            lines.push(format!("diff --jails {verb} {path}"));
            lines.extend(diff.lines().map(str::to_string));
        }
    };
    for operation in &bundle.plan.operations {
        match operation {
            Op::PublishMergedTree { before, after, .. } => {
                let entries = |digest: Option<&jails_contracts::ContentDigest>| {
                    digest
                        .and_then(|digest| bundle.trees.get(digest))
                        .map(|tree| tree.entries.clone())
                        .unwrap_or_default()
                };
                let was = entries(before.as_ref());
                let now = entries(Some(after));
                for path in was
                    .keys()
                    .chain(now.keys())
                    .collect::<std::collections::BTreeSet<_>>()
                {
                    let (old, new) = (was.get(path), now.get(path));
                    let verb = match (old.is_some(), new.is_some()) {
                        (false, _) => "create",
                        (true, true) => "replace",
                        (true, false) => "delete",
                    };
                    show(
                        verb,
                        path.as_str(),
                        old.and_then(|entry| text(&entry.blob)),
                        new.and_then(|entry| text(&entry.blob)),
                    );
                }
            }
            Op::ReplaceModelFile {
                path,
                before,
                after,
            }
            | Op::ReplaceStateFile {
                path,
                before,
                after,
            }
            | Op::PatchReaderFile {
                path,
                before,
                after,
            } => show(
                match before.is_some() {
                    true => "replace",
                    false => "create",
                },
                path.as_str(),
                before.as_ref().and_then(|image| text(&image.blob)),
                text(&after.blob),
            ),
            Op::AppendMigration { path, after } => {
                show("create", path.as_str(), None, text(&after.blob));
            }
            Op::RemoveReaderFile { path, before } => {
                show("delete", path.as_str(), text(&before.blob), None);
            }
        }
    }
    lines
}
