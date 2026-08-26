//! How a parsed command becomes a transition, and a transition a report.
//!
//! It shipped as `invoke` for two years because `jails-java` already has a
//! `dispatch` -- the splice that registers a generated command in a project's
//! own CLI -- and every architecture gate identified a file by its basename, so
//! two modules sharing a name were measured against each other's rules. That
//! made a *test* the reason this file was not called what it is.
//! `tests/architecture.rs`'s `module_of` answers `(crate, module)` now, and
//! `pending.md` §10.3 is the entry about it.
//!
//! Split out of `main.rs` under the ladder's largest-module gate, and the cut
//! is a real seam rather than a size one: `main.rs` says what the CLI
//! *accepts* -- the clap definition, one arm per subcommand -- and this says
//! what happens to a mutating one. Every route goes through [`mutate`], so
//! `--pretend`, `--debug`, `--no-start` and `--output` are honoured in exactly
//! one place and a command cannot forget one.

use crate::{Capability, Invocation, Output, add, model};
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
/// when planning stopped before it could produce an [`Outcome`].
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
pub(crate) fn mutate(
    invocation: Invocation,
    no_start: bool,
    route: impl Fn(&jails_engine::route::Run) -> Result<jails_engine::route::Outcome>,
) -> Result<()> {
    mutate_confirmed(invocation, no_start, true, route)
}

/// Apply a plan when argv intentionally contains no semantic command input.
pub(crate) fn apply_plan(invocation: Invocation) -> Result<()> {
    mutate(invocation, false, |_| {
        unreachable!("plan-in is handled before a semantic route is called")
    })
}

/// The same, with the confirmation a destructive command asks for first.
///
/// `confirmed = false` means "ask before committing, unless `--force`", and
/// the question is asked of the **plan**: the same computation the commit
/// runs, stopped one step before the lock. So what the reader is shown is
/// exactly what happens if they say yes -- not a second description of it, and
/// not a list assembled by whichever command happened to be destructive.
///
/// V1 asked from inside `destroy` and again from inside `remove`, over
/// hand-built path lists. Two implementations of one question, and neither
/// could see what the other command would do.
pub(crate) fn mutate_confirmed(
    invocation: Invocation,
    no_start: bool,
    assumed: bool,
    route: impl Fn(&jails_engine::route::Run) -> Result<jails_engine::route::Outcome>,
) -> Result<()> {
    let discovering = std::time::Instant::now();
    let project = model::Project::discover()?;
    let discover_time = discovering.elapsed();
    // Finish an interrupted transaction before any route reads the store.
    //
    // The executor recovers under the lock as well, but from there it can only
    // tell the caller its plan is stale -- and the routes that build their own
    // change set commit it directly, with no replan loop to catch that. So a
    // torn write left every subsequent command answering "run it again" while
    // the tear stayed on disk. One recovery here settles the project for
    // whatever plans next, and it still rides in the envelope, so `--output
    // json` says what was finished.
    let recovered = if invocation.pretend {
        Vec::new()
    } else {
        jails_engine::route::finish_interrupted(&project)?
    };
    fn configure(
        mut run: jails_engine::route::Run<'_>,
        no_start: bool,
        debug: bool,
    ) -> jails_engine::route::Run<'_> {
        if no_start {
            run = run.without_start();
        }
        if debug {
            run = run.with_debug();
        }
        run
    }
    if let Some(path) = &invocation.plan_in {
        let bytes = read_plan(path)?;
        let outcome = jails_engine::route::apply_plan(
            &configure(
                jails_engine::route::Run::committing(&project)
                    .with_timing(jails_prepare::timing::TimingPhase::Discover, discover_time),
                no_start,
                invocation.debug,
            ),
            &bytes,
        )?
        .after_recovery(recovered);
        drop_compiled_shadows(&project, &outcome);
        return report(
            &outcome,
            &invocation.command_path,
            invocation.output,
            invocation.review(),
            invocation.debug,
        );
    }

    let must_prepare = invocation.plan_out.is_some() || (!assumed && !invocation.pretend);
    let prepared = if must_prepare {
        Some(route(&configure(
            jails_engine::route::Run::pretending(&project)
                .with_timing(jails_prepare::timing::TimingPhase::Discover, discover_time),
            no_start,
            invocation.debug,
        ))?)
    } else {
        None
    };
    if !assumed
        && !invocation.pretend
        && !accepted(
            prepared.as_ref().expect("confirmation prepared above"),
            invocation.review(),
            invocation.debug,
        )?
    {
        println!("aborted");
        return Ok(());
    }

    let portable = match (&invocation.plan_out, prepared.as_ref()) {
        (Some(path), Some(planned)) => {
            let bytes = planned.portable_plan()?;
            jails_support::apply::put_outside_project_private_atomic(path, &bytes)?;
            Some(bytes)
        }
        _ => None,
    };
    if invocation.pretend {
        let outcome = match prepared {
            Some(outcome) => outcome,
            None => route(&configure(
                jails_engine::route::Run::pretending(&project)
                    .with_timing(jails_prepare::timing::TimingPhase::Discover, discover_time),
                no_start,
                invocation.debug,
            ))?,
        };
        return report(
            &outcome,
            &invocation.command_path,
            invocation.output,
            invocation.review(),
            invocation.debug,
        );
    }

    let run = configure(
        jails_engine::route::Run::committing(&project)
            .with_timing(jails_prepare::timing::TimingPhase::Discover, discover_time),
        no_start,
        invocation.debug,
    );
    let routed = match portable {
        Some(bytes) => jails_engine::route::apply_plan(&run, &bytes),
        None => route(&run),
    };
    // A refusal after recovery is still a refusal, but the reader has to be
    // told what the project just did on its own -- otherwise the command that
    // finished an interrupted transaction reports only that the component it
    // was asked to add is already there, which reads as nothing happening.
    // On the success path the recovery rides in the envelope instead, so it is
    // said once either way and `--output json` carries it.
    let outcome = match routed {
        Ok(outcome) => outcome.after_recovery(recovered),
        Err(error) => return Err(said_after_recovery(error, &recovered)),
    };
    drop_compiled_shadows(&project, &outcome);
    report(
        &outcome,
        &invocation.command_path,
        invocation.output,
        invocation.review(),
        invocation.debug,
    )
}

/// A failure, prefixed with what recovery finished before it.
fn said_after_recovery(
    error: jails_support::Failure,
    recovered: &[jails_prepare::recovery::RecoveryOutcome],
) -> jails_support::Failure {
    let lines: Vec<String> = recovered
        .iter()
        .flat_map(|outcome| outcome.changes.iter())
        .map(|change| format!("recovered {}", jails_prepare::report::recovery_line(change)))
        .collect();
    match (lines.is_empty(), error.message()) {
        (true, _) | (_, None) => error,
        (false, Some(message)) => {
            jails_support::Failure::Told(format!("{}\n{message}", lines.join("\n")))
        }
    }
}

fn read_plan(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect prepared plan `{}`: {error}",
            path.display()
        )
    })?;
    let cap = (jails_support::codec::MAX_PROTOCOL_RECORD as u64)
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(1024 * 1024))
        .expect("the protocol record cap fits u64");
    if metadata.len() > cap {
        return Err(format!(
            "prepared plan `{}` is {} bytes, over the {cap}-byte limit.\n       \
             fix: discard the oversized file and export the plan again.",
            path.display(),
            metadata.len()
        )
        .into());
    }
    Ok(std::fs::read(path)
        .map_err(|error| format!("failed to read prepared plan `{}`: {error}", path.display()))?)
}

/// Run a mutation the reader did not ask for, and say nothing if it was
/// already done.
///
/// One caller: `jails test --fast` needs JUnit's console launcher on the test
/// classpath, and that is a dependency in the reader's POM -- so it is an
/// owned entity installed by an ordinary transition rather than a splice from
/// inside the test runner. The first `--fast` reports it like any other
/// mutation; every later one changes nothing, and `nothing to do` printed
/// before every test run would be noise about something nobody typed.
///
/// The silence is only over an *empty* outcome. A failure still fails, and a
/// transition that writes anything still reports what it wrote.
pub(crate) fn precondition(
    invocation: Invocation,
    route: impl Fn(&jails_engine::route::Run) -> Result<jails_engine::route::Outcome>,
) -> Result<()> {
    let discovering = std::time::Instant::now();
    let project = model::Project::discover()?;
    let discover_time = discovering.elapsed();
    let mut run = match invocation.pretend {
        true => jails_engine::route::Run::pretending(&project),
        false => jails_engine::route::Run::committing(&project),
    }
    .with_timing(jails_prepare::timing::TimingPhase::Discover, discover_time);
    if invocation.debug {
        run = run.with_debug();
    }
    let outcome = route(&run)?;
    match outcome.operations().is_empty() {
        true => Ok(()),
        false => report(
            &outcome,
            &invocation.command_path,
            invocation.output,
            invocation.review(),
            invocation.debug,
        ),
    }
}

/// Show what a plan would delete and ask, or say yes for a plan that deletes
/// nothing.
///
/// Only deletions are put to the reader. A create or a replace is what they
/// asked for; a delete is the one operation that loses something they cannot
/// get back from this command.
fn accepted(
    planned: &jails_engine::route::Outcome,
    review: jails_prepare::review::ReviewSelection,
    debug: bool,
) -> Result<bool> {
    use std::io::{BufRead, Write};

    let deleting: Vec<String> = planned
        .operations()
        .iter()
        .filter(|op| op.kind == jails_prepare::report::ReportedOpKind::Delete)
        .map(|op| op.path.to_string())
        .collect();
    if deleting.is_empty() {
        return Ok(true);
    }
    if review.any() {
        print!(
            "{}",
            jails_prepare::review::render_human(planned.review(), review)
        );
    }
    if debug {
        print!(
            "{}",
            jails_prepare::report::render_timings(&planned.timings())
        );
    }
    println!("about to delete:");
    for path in &deleting {
        println!("  {path}");
    }
    print!("proceed? [y/N] ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|error| format!("failed to read confirmation: {error}"))?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

/// Take a deleted source file's compiled shadow with it.
///
/// Derived from the receipt rather than from what the command was asked to do,
/// so it covers every route without any of them knowing about `target/`. It is
/// deliberately **not** part of the transaction: `target/` is build output,
/// nothing guards it, and a transition that rewrote it would be claiming
/// ownership of something Maven owns.
///
/// What it prevents is narrow and real. `mvn test` is incremental, so a
/// deleted `TestcontainersConfig.java` whose `.class` is still under
/// `target/test-classes` goes on being loaded -- the removal looks like it did
/// not happen, and the failure surfaces in a test run rather than at the
/// command that caused it.
fn drop_compiled_shadows(project: &model::Project, outcome: &jails_engine::route::Outcome) {
    for deleted in outcome.deleted_files() {
        add::drop_compiled_shadow(project, std::path::Path::new(&deleted));
    }
}

/// One `Declaration` per capability the caller named.
///
/// `--name` and `--package` apply to every capability in the invocation,
/// which is what makes `jails add db kafka --name orders` mean two named
/// instances rather than one named and one anonymous.
pub(crate) fn declarations(
    capabilities: &[Capability],
    name: Option<&str>,
    package: Option<&str>,
) -> Result<Vec<jails_project::capability::Declaration>> {
    capabilities
        .iter()
        .map(|capability| {
            let asked = jails_project::capability::Declaration::asked(*capability, name, package);
            asked.validate()?;
            Ok(asked)
        })
        .collect()
}

/// Each capability is its own transition, and each reports.
///
/// Not one transition over the list: `add db kafka` is two entities, and a
/// scope speaking for both would relinquish one when the other is the subject.
/// The loop stops at the first refusal, so a project is never left with half
/// of what the reader asked for and no word about which half.
pub(crate) fn one_transition_each(
    run: &jails_engine::route::Run,
    asked: &[jails_project::capability::Declaration],
    route: impl Fn(
        &jails_engine::route::Run,
        &jails_project::capability::Declaration,
    ) -> Result<jails_engine::route::Outcome>,
) -> Result<jails_engine::route::Outcome> {
    let mut last = None;
    for declaration in asked {
        // Re-resolved between transitions, not once for the loop. A capability
        // can render differently because of what an earlier one installed --
        // `add api`'s advice gains a `DuplicateKeyException` arm only when the
        // JDBC starter is there, and `add db` is what puts it there -- and a
        // `Project` resolved before the first commit describes a project that
        // stopped existing. `pending.md` §1.1.
        //
        // On `--pretend` this re-reads a project nothing wrote to, which costs
        // a few file reads and keeps one code path.
        let project = model::Project::load(run.project().root())?;
        last = Some(route(&run.against(&project), declaration)?);
    }
    Ok(last.ok_or_else(|| {
        "name at least one capability.\n       fix: `jails add --help` lists them.".to_string()
    })?)
}

/// The one rendering of what a mutation did.
///
/// The exit code comes from the envelope's own status, so a conflicted apply
/// and a refusal are distinguishable by a script without parsing prose. A
/// nonzero status returns an **empty** `Err`, the same convention `doctor`
/// uses: the report has already been printed and a second `jails: ` line over
/// it would say nothing.
pub(crate) fn report(
    outcome: &jails_engine::route::Outcome,
    command_path: &[String],
    output: Output,
    review: jails_prepare::review::ReviewSelection,
    debug: bool,
) -> Result<()> {
    let Some(envelope) = outcome.envelope() else {
        // §R4.3 makes an incomplete commit a success-side value carrying what
        // is known, and it has no single status yet. Saying so is better than
        // inventing one: the transaction is on disk and the next invocation
        // finishes it.
        return Err(jails_support::Failure::Told(
            concat!(
                "the commit was recorded and then left work behind. Nothing is lost -- the ",
                "transaction is on disk.\n       fix: run the same command again; it finishes ",
                "the interrupted one before doing anything new.",
            )
            .to_string(),
        ));
    };
    let rendered = match output {
        Output::Human => {
            let mut rendered = jails_prepare::report::render_envelope(&envelope);
            if review.any() {
                rendered.push_str(&jails_prepare::review::render_human(
                    outcome.review(),
                    review,
                ));
            }
            if debug {
                rendered.push_str(&jails_prepare::report::render_timings(&envelope.timings));
            }
            rendered
        }
        Output::Json => {
            let fingerprint = outcome.request_fingerprint().ok_or_else(|| {
                "mutation result omitted its canonical request fingerprint".to_string()
            })?;
            let envelope = jails_prepare::command::CommandEnvelopeV2::from_v1(
                jails_prepare::command::CommandIdentity {
                    path: command_path.to_vec(),
                    fingerprint,
                    read_only: false,
                },
                &envelope,
            );
            jails_prepare::serialize::envelope_v2_with_review(
                &envelope,
                review.any().then(|| (outcome.review(), review)),
            )
        }
        Output::JsonV1 => jails_prepare::serialize::envelope(&envelope),
    };
    print!("{rendered}");
    // A preview reads exactly like a commit -- one line per operation, in the
    // executor's order -- which is the whole point and also the one thing that
    // could be misread. Said once, here, rather than by each route.
    if matches!(outcome, jails_engine::route::Outcome::Planned(_)) && output == Output::Human {
        println!("\nnothing was written -- run the same command without --pretend to apply it.");
    }
    match envelope.exit_code() {
        0 => Ok(()),
        _ => Err(jails_support::Failure::Reported),
    }
}
