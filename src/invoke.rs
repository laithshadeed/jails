//! How a parsed command becomes a transition, and a transition a report.
//!
//! Named `invoke` rather than `dispatch` because `jails-java` already has a
//! `dispatch` -- the splice that registers a generated command in a project's
//! own CLI -- and every architecture gate identifies a file by its first path
//! component, so two modules sharing a name are measured against each other's
//! rules.
//!
//! Split out of `main.rs` under the ladder's largest-module gate, and the cut
//! is a real seam rather than a size one: `main.rs` says what the CLI
//! *accepts* -- the clap definition, one arm per subcommand -- and this says
//! what happens to a mutating one. Every route goes through [`mutate`], so
//! `--pretend`, `--debug`, `--no-start` and `--output` are honoured in exactly
//! one place and a command cannot forget one.

use crate::{Capability, Invocation, Output, add, model};
use jails_generate::generate::ArtifactKind;
use jails_support::Result;

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
    let project = model::Project::discover()?;
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
    if !assumed && !invocation.pretend {
        let planned = route(&configure(
            jails_engine::route::Run::pretending(&project),
            no_start,
            invocation.debug,
        ))?;
        if !accepted(&planned)? {
            println!("aborted");
            return Ok(());
        }
    }
    let run = configure(
        match invocation.pretend {
            true => jails_engine::route::Run::pretending(&project),
            false => jails_engine::route::Run::committing(&project),
        },
        no_start,
        invocation.debug,
    );
    let outcome = route(&run)?;
    drop_compiled_shadows(&project, &outcome);
    report(&outcome, invocation.output)
}

/// Show what a plan would delete and ask, or say yes for a plan that deletes
/// nothing.
///
/// Only deletions are put to the reader. A create or a replace is what they
/// asked for; a delete is the one operation that loses something they cannot
/// get back from this command.
fn accepted(planned: &jails_engine::route::Outcome) -> Result<bool> {
    use std::io::{BufRead, Write};

    let deleting: Vec<String> = planned
        .operations()
        .iter()
        .filter(|op| op.kind == jails_prepare::report::ReportedOpKind::Delete)
        .map(|op| match &op.path {
            jails_prepare::prepare::OperationTarget::Project(path) => path.to_string(),
            jails_prepare::prepare::OperationTarget::LegacyMachine(path) => {
                format!("legacy-machine {path:?}")
            }
        })
        .collect();
    if deleting.is_empty() {
        return Ok(true);
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

/// `<kind>:<Name>`, as `--intent` takes it.
///
/// Two segments, not three. The key already names one exact decoded row, and
/// the row carries its own package -- a third segment would be a second
/// opinion about the same fact, and the interesting case is the two
/// disagreeing.
pub(crate) fn parse_intent(intent: &str) -> Result<(ArtifactKind, String)> {
    let mut parts = intent.splitn(3, ':');
    let (Some(kind), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "`--intent {intent}` is not `<kind>:<Name>`.\n       fix: `jails doctor` prints the \
             exact command for every row this project can adopt."
        ));
    };
    let kind = <ArtifactKind as clap::ValueEnum>::from_str(kind, false).map_err(|_| {
        format!(
            "`{kind}` is not a generator kind.\n       fix: `jails commands --json` lists every \
             kind this binary accepts."
        )
    })?;
    Ok((kind, name.to_string()))
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
        last = Some(route(run, declaration)?);
    }
    last.ok_or_else(|| {
        "name at least one capability.\n       fix: `jails add --help` lists them.".to_string()
    })
}

/// The one rendering of what a mutation did.
///
/// The exit code comes from the envelope's own status, so a conflicted apply
/// and a refusal are distinguishable by a script without parsing prose. A
/// nonzero status returns an **empty** `Err`, the same convention `doctor`
/// uses: the report has already been printed and a second `jails: ` line over
/// it would say nothing.
pub(crate) fn report(outcome: &jails_engine::route::Outcome, output: Output) -> Result<()> {
    let Some(envelope) = outcome.envelope() else {
        // §R4.3 makes an incomplete commit a success-side value carrying what
        // is known, and it has no single status yet. Saying so is better than
        // inventing one: the transaction is on disk and the next invocation
        // finishes it.
        return Err(concat!(
            "the commit was recorded and then left work behind. Nothing is lost -- the ",
            "transaction is on disk.\n       fix: run the same command again; it finishes ",
            "the interrupted one before doing anything new.",
        )
        .to_string());
    };
    print!(
        "{}",
        match output {
            Output::Human => jails_prepare::report::render_envelope(&envelope),
            Output::Json => jails_prepare::report::render_envelope_json(&envelope),
        }
    );
    // A preview reads exactly like a commit -- one line per operation, in the
    // executor's order -- which is the whole point and also the one thing that
    // could be misread. Said once, here, rather than by each route.
    if matches!(outcome, jails_engine::route::Outcome::Planned(_)) && output == Output::Human {
        println!("\nnothing was written -- run the same command without --pretend to apply it.");
    }
    match envelope.exit_code() {
        0 => Ok(()),
        _ => Err(String::new()),
    }
}
