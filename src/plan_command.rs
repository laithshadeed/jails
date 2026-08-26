//! The argv seam for applying an already prepared plan.

use crate::{Invocation, Output};
use jails_support::Result;

pub(crate) fn requested() -> Option<Result<()>> {
    match invocation() {
        Ok(Some(invocation)) => Some(crate::dispatch::apply_plan(invocation)),
        Ok(None) => None,
        Err(failure) => Some(Err(failure)),
    }
}

/// Recognise a plan import before clap requires the semantic arguments that
/// originally produced it.
///
/// `generate scaffold --plan-in plan.json` intentionally has no resource
/// name or fields: those values are authenticated inside the plan and parsing
/// them again would create a second source of intent. Only presentation flags
/// are read here; all other argv is inert command-path spelling.
pub(crate) fn invocation() -> Result<Option<Invocation>> {
    let mut args = std::env::args_os().skip(1).peekable();
    let mut plan_in = None;
    let mut debug = false;
    let mut output = Output::Human;
    let mut diff = false;
    let mut ast = false;
    let mut pretend = false;
    let mut plan_out = false;
    while let Some(argument) = args.next() {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "--plan-in" => {
                let path = args.next().ok_or(concat!(
                    "--plan-in requires a file path.\n       ",
                    "fix: pass the mode-0600 plan file written by --plan-out."
                ))?;
                if plan_in.replace(std::path::PathBuf::from(path)).is_some() {
                    return Err(concat!(
                        "--plan-in may be supplied only once.\n       ",
                        "fix: remove the duplicate --plan-in option."
                    )
                    .into());
                }
            }
            "--debug" => debug = true,
            "--diff" => diff = true,
            "--ast" => ast = true,
            "--pretend" | "--dry-run" | "-p" => pretend = true,
            "--plan-out" => {
                plan_out = true;
                let _ = args.next().ok_or(concat!(
                    "--plan-out requires a file path.\n       ",
                    "fix: pass the private file that should receive the plan."
                ))?;
            }
            "--output" => {
                let value = args.next().ok_or(concat!(
                    "--output requires human, json, or json-v1.\n       ",
                    "fix: pass `--output human`, `--output json`, or `--output json-v1`."
                ))?;
                output = parse_output(&value.to_string_lossy())?;
            }
            _ => {
                if let Some(path) = text.strip_prefix("--plan-in=") {
                    if path.is_empty() {
                        return Err(concat!(
                            "--plan-in requires a file path.\n       ",
                            "fix: pass the mode-0600 plan file written by --plan-out."
                        )
                        .into());
                    }
                    if plan_in.replace(std::path::PathBuf::from(path)).is_some() {
                        return Err(concat!(
                            "--plan-in may be supplied only once.\n       ",
                            "fix: remove the duplicate --plan-in option."
                        )
                        .into());
                    }
                } else if text.starts_with("--plan-out=") {
                    plan_out = true;
                } else if let Some(value) = text.strip_prefix("--output=") {
                    output = parse_output(value)?;
                }
            }
        }
    }
    if plan_in.is_some() && pretend {
        return Err(concat!(
            "--plan-in cannot be combined with --pretend.\n       ",
            "fix: remove --pretend; importing already applies the reviewed plan."
        )
        .into());
    }
    if plan_in.is_some() && plan_out {
        return Err(concat!(
            "--plan-in cannot be combined with --plan-out.\n       ",
            "fix: choose one existing plan to import or one new plan to export."
        )
        .into());
    }
    Ok(plan_in.map(|path| Invocation {
        pretend: false,
        debug,
        output,
        diff,
        ast,
        plan_out: None,
        plan_in: Some(path),
        command_path: crate::cli::command_path_from_env(),
    }))
}

fn parse_output(value: &str) -> Result<Output> {
    match value {
        "human" => Ok(Output::Human),
        "json" => Ok(Output::Json),
        "json-v1" => Ok(Output::JsonV1),
        other => Err(format!(
            "unsupported --output `{other}`; expected human, json, or json-v1.\n       \
             fix: pass `--output human`, `--output json`, or `--output json-v1`."
        )
        .into()),
    }
}
