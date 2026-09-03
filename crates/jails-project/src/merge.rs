//! Three-way text merge for generated files: base is the accepted compiler
//! projection, current is the captured workspace, desired is the next render.

use jails_contracts::ProjectPath;
use jails_model::Diagnostic;
use jails_support::hermetic::{self, Invocation, Outcome};
use jails_support::scratch::ScratchDir;
use jails_support::{hex, sha256};
use std::time::Duration;

pub enum Merged {
    Clean(Vec<u8>),
    Conflicted { hunks: usize },
}

pub fn three_way(
    path: &ProjectPath,
    base: &[u8],
    current: &[u8],
    desired: &[u8],
) -> Result<Merged, Diagnostic> {
    for (side, bytes) in [("base", base), ("current", current), ("desired", desired)] {
        if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
            return Err(Diagnostic::new(
                "workspace-merge-binary",
                path.to_string(),
                format!(
                    "`{path}` has a {side} image that is not UTF-8 text; binary generated files cannot be merged"
                ),
                "restore text source or resolve the file by hand",
            ));
        }
    }
    let key = hex(&sha256(path.as_str().as_bytes()));
    let scratch = ScratchDir::in_temp("jails-canonical-merge")
        .map_err(|error| scratch_failed(path, &error))?;
    let inputs = scratch.path().join("inputs").join(&key);
    for (name, bytes) in [("current", current), ("base", base), ("desired", desired)] {
        jails_support::apply::put_in_scratch(inputs.join(name), bytes)
            .map_err(|error| scratch_failed(path, &error))?;
    }
    let run = hermetic::run(&Invocation {
        program: "git".into(),
        args: jails_support::git::merge_file_argv(
            &["--no-diff3"],
            [
                "-L".into(),
                "current".into(),
                "-L".into(),
                "base".into(),
                "-L".into(),
                "jails-desired".into(),
                "current".into(),
                "base".into(),
                "desired".into(),
            ],
        ),
        working_directory: inputs,
        environment: Invocation::minimal_environment(
            std::env::var("PATH").as_deref().unwrap_or("/usr/bin:/bin"),
            &[],
        ),
        timeout: Duration::from_secs(30),
    })
    .map_err(|error| {
        Diagnostic::without_a_fix(
            "workspace-merge-tool-unavailable",
            path.to_string(),
            format!("`{path}` changed on both sides and needs git merge-file: {error}"),
        )
    })?;
    let output = run.stdout.bytes;
    scratch
        .close()
        .map_err(|error| scratch_failed(path, &error))?;
    if run.stdout.truncated {
        return Err(Diagnostic::new(
            "workspace-merge-output-truncated",
            path.to_string(),
            format!("`{path}` produced too much merge output"),
            "simplify the file or resolve it by hand",
        ));
    }
    match run.outcome {
        Outcome::Exited { code: 0 } => Ok(Merged::Clean(output)),
        Outcome::Exited { code } if (1..=127).contains(&code) => {
            let text = String::from_utf8(output).map_err(|_| {
                Diagnostic::without_a_fix(
                    "workspace-merge-output-not-utf8",
                    path.to_string(),
                    format!("`{path}`: git produced non-UTF-8 conflict output"),
                )
            })?;
            let hunks = text
                .lines()
                .filter(|line| *line == "<<<<<<< current")
                .count();
            if hunks == 0 {
                return Err(Diagnostic::new(
                    "workspace-merge-markers-missing",
                    path.to_string(),
                    format!("`{path}`: git reported a conflict without conflict markers"),
                    "resolve the file by hand",
                ));
            }
            Ok(Merged::Conflicted { hunks })
        }
        other => Err(Diagnostic::new(
            "workspace-merge-tool-failed",
            path.to_string(),
            format!("`{path}`: git merge-file failed as {other:?}"),
            format!(
                "verify git merge-file works or resolve the file by hand{}",
                jails_support::git::pinned_algorithm_hint().unwrap_or_default()
            ),
        )),
    }
}

/// The scratch directory the merge runs in could not be made, filled or
/// removed. One code, because all three are the same machine saying no and
/// the message is whatever it said.
fn scratch_failed(path: &ProjectPath, error: &impl std::fmt::Display) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-merge-scratch",
        path.to_string(),
        error.to_string(),
    )
}
