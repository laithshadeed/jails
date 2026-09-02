//! How a parsed command becomes a transition, and a transition a report.
//!
//! `main.rs` says what the CLI *accepts* -- the clap definition, one arm per
//! subcommand -- and this says what happens to the result of one.
//!
//! **The flags ride on [`Invocation`], which `main` builds once.**
//! `--pretend`, `--debug`, `--output`, `--diff`, `--ast` and the plan paths
//! are read from the parsed CLI into that one value, so a route receives them
//! rather than re-reading them and a command cannot honour a different set
//! from its neighbour. This module is the other end: it turns whatever a route
//! returns into the process's exit status and its single rendered report.

use crate::{Invocation, Output};
use jails_support::Result;

/// Convert the command result to the process protocol once.
pub(crate) fn finish(result: Result<()>) -> std::process::ExitCode {
    if let Err(failure) = result {
        // A reported failure already printed its structured result. Printing
        // a second empty `jails:` line would obscure rather than explain it.
        if let Some(message) = failure.message() {
            eprintln!("jails: {message}");
        }
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

/// Finish a parsed invocation, preserving the selected machine encoding even
/// when planning stopped before it could produce a `route::Outcome`.
pub(crate) fn finish_invocation(
    result: Result<()>,
    output: Output,
    command_path: &[String],
) -> std::process::ExitCode {
    let Err(failure) = result else {
        return std::process::ExitCode::SUCCESS;
    };
    let Some(message) = failure.message() else {
        return std::process::ExitCode::FAILURE;
    };
    if output == Output::Human {
        eprintln!("jails: {message}");
        return std::process::ExitCode::FAILURE;
    }

    let schema = match output {
        Output::Human => unreachable!("handled above"),
        Output::JsonV1 => "jails.command-result.v1",
        Output::Json => "jails.command-result.v2",
    };
    let envelope = serde_json::json!({
        "schema": schema,
        "status": "refused",
        "exit_code": 1,
        "command": command_path,
        "error": { "code": "invalid-request", "message": message },
    });
    let rendered = format!("{envelope}\n");
    print!("{rendered}");
    std::process::ExitCode::FAILURE
}

/// Apply a plan bundle written earlier by `--plan-out`.
///
/// **The reviewed transition is the bundle, and applying it never replans.**
/// That is `PlanBundle`'s contract, so this is the same call `jails model
/// apply` makes: read the file, hand it to the one executor, report what it
/// did.
pub(crate) fn apply_plan(invocation: Invocation) -> Result<()> {
    let path = invocation.plan_in.clone().ok_or_else(|| {
        jails_support::Failure::Told("--plan-in requires a file path".to_string())
    })?;
    crate::model_command::apply(&path, invocation.output)
}
