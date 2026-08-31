//! How a parsed command becomes a transition, and a transition a report.
//!
//! It shipped as `invoke` for two years because `jails-java` already has a
//! `dispatch` -- the splice that registers a generated command in a project's
//! own CLI -- and every architecture gate identified a file by its basename, so
//! two modules sharing a name were measured against each other's rules. That
//! made a *test* the reason this file was not called what it is.
//! `tests/architecture/`'s `module_of` answers `(crate, module)` now, and
//! `pending.md` §10.3 is the entry about it.
//!
//! Split out of `main.rs` under the ladder's largest-module gate, and the cut
//! is a real seam rather than a size one: `main.rs` says what the CLI
//! *accepts* -- the clap definition, one arm per subcommand -- and this says
//! what happens to a mutating one. Every route goes through [`mutate`], so
//! `--pretend`, `--debug`, `--no-start` and `--output` are honoured in exactly
//! one place and a command cannot forget one.

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

    let syntax = jails_protocol::request::CanonicalRequestSyntaxV1 {
        command_path: command_path.to_vec(),
        ..Default::default()
    };
    let fingerprint = match syntax.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            eprintln!("jails: {message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let envelope =
        jails_prepare::command::CommandEnvelope::refused(jails_prepare::command::ErrorReport::new(
            jails_prepare::command::ErrorCode::InvalidRequest,
            message,
        ));
    let rendered = match output {
        Output::Human => unreachable!("handled above"),
        Output::JsonV1 => jails_prepare::serialize::envelope(&envelope),
        Output::Json => {
            let envelope = jails_prepare::command::CommandEnvelopeV2::from_v1(
                jails_prepare::command::CommandIdentity {
                    path: command_path.to_vec(),
                    fingerprint,
                    read_only: false,
                },
                &envelope,
            );
            jails_prepare::serialize::envelope_v2(&envelope)
        }
    };
    print!("{rendered}");
    std::process::ExitCode::FAILURE
}

/// Run one mutation through the transaction protocol, and report it once.
///
/// **Every mutating command goes through here.** That is the point of the
/// single dispatch point plan.md §R6 names: `--pretend`, `--debug`,
/// `--no-start` and `--output` are honoured in one place, so a command cannot
/// forget one, and the result is rendered from the envelope rather than
/// printed as the route goes. A route that printed its own progress would be
/// describing the work a second time, which is the drift §R3.4 exists to
/// remove.
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
