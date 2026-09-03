//! What is left to do once the transition is durable.
//!
//! **Split out by the one thing everything here has in common: it runs after
//! the executor.** Nothing in this module decides what the project should
//! become -- that is settled by the time any of it is called -- and nothing in
//! it can fail the transition, because the files are already written. So each
//! is best-effort and says so on the way past: a machine with no formatter
//! gets a note, a machine with no container engine gets a line telling it how
//! to start the services later.
//!
//! What rides on the *plan* rather than being decided here is which effects
//! there are. `--pretend` shows them, the exported bundle carries them, and
//! apply cannot start something the reviewed plan did not name.

use crate::{Invocation, Output};
use jails_support::{Failure, Result};

/// The compiled shadow of every source this transition deleted.
///
/// **A `.class` outlives its source, and the build does not notice.** `mvn
/// test` is incremental: a class left under `target/test-classes` after its
/// `.java` is gone goes on being loaded and run, so a `remove db` that took
/// `TestcontainersConfig.java` away leaves every `@SpringBootTest` still
/// starting a container -- and the removal looks like it did not happen, on a
/// green build.
///
/// It is not in the plan because it is not a project write: `apply::
/// remove_derived` refuses any path outside `target/` or `build/`, so the
/// exemption is checked rather than promised. Best effort, and silent -- a
/// project that has never been compiled has nothing here, which is the common
/// case rather than a failure.
fn drop_compiled_shadows(root: &std::path::Path, bundle: &jails_contracts::PlanBundle) {
    // Where javac put it, per build tool and source set. Both are swept
    // because which one this project uses is not worth a second observation
    // for a directory that either exists or does not.
    const OUTPUTS: [(&str, &str); 4] = [
        ("main", "target/classes"),
        ("test", "target/test-classes"),
        ("main", "build/classes/java/main"),
        ("test", "build/classes/java/test"),
    ];
    for path in crate::plan_delta::deleted_paths(bundle) {
        let text = path.as_str();
        let Some(name) = text.strip_suffix(".java") else {
            continue;
        };
        // The package path, whichever source set the file was in.
        let Some(relative) = ["src/main/java/", "src/test/java/"]
            .iter()
            .find_map(|prefix| name.strip_prefix(prefix))
        else {
            continue;
        };
        let set = if text.contains("/test/") {
            "test"
        } else {
            "main"
        };
        for (source_set, output) in OUTPUTS {
            if source_set != set {
                continue;
            }
            let compiled = root.join(output).join(relative);
            let _ = jails_support::apply::remove_derived(compiled.with_extension("class"));
            // Nested and anonymous classes compile to siblings named
            // `Outer$Inner.class`, and one left behind is as loadable as the
            // outer class would have been.
            let (Some(directory), Some(stem)) = (compiled.parent(), compiled.file_name()) else {
                continue;
            };
            let prefix = format!("{}$", stem.to_string_lossy());
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let nested = entry.file_name();
                let nested = nested.to_string_lossy();
                if nested.starts_with(&prefix) && nested.ends_with(".class") {
                    let _ = jails_support::apply::remove_derived(entry.path());
                }
            }
        }
    }
}

/// The exact compose document this plan wrote, on disk outside the project.
///
/// `None` when the bundle does not carry it, which is the caller's cue to fall
/// back to the live file rather than to skip the services.
fn stage_compose_document(
    bundle: &jails_contracts::PlanBundle,
    path: &str,
) -> Option<jails_support::scratch::ScratchDir> {
    use jails_contracts::PlannedOperation as Op;
    let digest = bundle
        .plan
        .operations
        .iter()
        .find_map(|operation| match operation {
            Op::PatchReaderFile {
                path: target,
                after,
                ..
            } if target.as_str() == path => Some(after.blob.clone()),
            _ => None,
        })?;
    let bytes = bundle.blobs.get(&digest)?;
    let staged = jails_support::scratch::ScratchDir::in_temp("compose").ok()?;
    let file = staged.path().join("compose.yaml");
    jails_support::apply::put_in_scratch(&file, bytes).ok()?;
    Some(staged)
}

/// Do what the reviewed plan said was left once the files were written.
///
/// **The effect is in the plan, not in this function's judgement.** A compose
/// service jails declares is not running because it was declared, and the
/// command that declared it is the one place a reader is looking -- so the
/// same command starts it, `--no-start` says not to, and the failure names
/// that flag. Reading the intent off the bundle rather than re-deciding here
/// is what makes `--pretend` and the exported bundle able to show it.
///
/// The files are already durable when this runs, so a failed effect is
/// reported as a failed *effect*: the status is 1 because the services really
/// are not up, and the message says the project itself is complete. Exiting 0
/// would be worse -- `for c in db api; do jails add $c || fail; done` is how
/// people write this, and a silent half-install is what it would hide.
/// A formatter run a batched mutation left for its caller.
///
/// Process-wide because a replay is one process: every row that would have
/// formatted sets it, and the replay's last step runs the formatter once if
/// any row did. Cleared by that run, so a second replay in the same process
/// starts owing nothing.
static OWED_FORMAT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Run the formatter the batched rows owed, once, if any of them did.
pub(crate) fn run_owed_format(invocation: &Invocation) -> Result<()> {
    if OWED_FORMAT.swap(false, std::sync::atomic::Ordering::Relaxed) {
        jails_drive::run::format_generated(&invocation.root()?, invocation.debug);
    }
    Ok(())
}

pub(crate) fn run_follow_up_effects(
    root: &std::path::Path,
    bundle: &jails_contracts::PlanBundle,
    execution: &jails_workspace::Execution,
    invocation: &Invocation,
) -> Result<()> {
    // **The formatter runs over what was just written, before anything
    // else.** A project that declares `format` fails `jails check` on jails'
    // own output otherwise: the wrapping a formatter chooses cannot be
    // predicted from a template, which is what a formatter is for. Best
    // effort, like every other tool jails shells out to -- a machine with no
    // Maven gets a note rather than a failed generation.
    //
    // **Only over a tree that changed.** The plan declares the effect from
    // the model, and a mutation that wrote nothing -- a second identical
    // command, a `set` of the value already there -- leaves nothing to
    // format; a Maven run for it is a JVM spent on a no-op. A replay owes
    // the run to its caller instead, which runs it once after the last row
    // (`run_owed_format`).
    drop_compiled_shadows(root, bundle);
    let formats = bundle
        .plan
        .follow_up_effects
        .iter()
        .any(|effect| effect.kind == "format")
        && execution.files_written > 0;
    if formats && invocation.batch_effects {
        OWED_FORMAT.store(true, std::sync::atomic::Ordering::Relaxed);
    } else if formats {
        jails_drive::run::format_generated(root, invocation.debug);
    }
    let compose: Vec<&jails_contracts::EffectIntent> = bundle
        .plan
        .follow_up_effects
        .iter()
        .filter(|effect| effect.kind == "compose-up")
        .collect();
    let services: Vec<&str> = compose
        .iter()
        .filter_map(|effect| effect.arguments.get("service").map(String::as_str))
        .collect();
    if services.is_empty() {
        return Ok(());
    }
    if invocation.no_start {
        if invocation.output == Output::Human {
            println!(
                "  waiting  {} -- run `jails start` when you want {} up",
                services.join(", "),
                if services.len() == 1 { "it" } else { "them" }
            );
        }
        return Ok(());
    }
    // **Against the bytes this transition published, not the live file.**
    // Between the commit and this call somebody may edit `compose.yaml`, and
    // running against what they wrote would start services the reviewed plan
    // never described. The document is staged outside the project so it is not
    // mistaken for one of its files; `--project-directory` keeps every
    // relative path in it resolving against the project. When the bundle does
    // not carry the document -- an older plan, or an export read back --
    // falling back to the live file is better than not starting at all.
    let staged = compose
        .iter()
        .filter_map(|effect| effect.arguments.get("document"))
        .next()
        .and_then(|path| stage_compose_document(bundle, path));
    let started = match staged.as_ref() {
        Some(staged) => {
            jails_project::compose::up_document(root, staged.path(), &services, invocation.debug)
        }
        None => jails_project::compose::up(root, &services, invocation.debug),
    };
    if started {
        return Ok(());
    }
    if invocation.output == Output::Human {
        println!("  {:<8}{}", "(failed)", services.join(", "));
        println!(
            "Every file this command wrote are written and durable; only the services are not up."
        );
        println!(
            "       fix: start the container engine and run `jails start`, or repeat with `--no-start`"
        );
    }
    Err(Failure::Reported)
}
