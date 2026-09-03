//! Bare `jails`: what `git status` prints, for a jails project.
//!
//! **Every fact here is one another command already computes**, assembled
//! from a single capture and nothing else. That is the whole design
//! constraint: a status a reader types by reflex has to answer before they
//! have finished letting go of the return key, so it starts no process,
//! probes no version and asks no container engine. What it costs is one
//! model parse and one capture -- the same capture `jails model status`
//! makes -- and on a thirty-declaration project that is about twenty
//! milliseconds.
//!
//! The line that is *not* here is the one the roadmap asked for: which
//! declared services are actually running. `docker info` is 165 ms on this
//! machine and must never be cached (an engine that stopped ten minutes ago
//! reports as up), so it would be eight times the whole budget for one line.
//! `compose` says what is declared and which command runs it; `jails doctor`
//! is where the machine gets asked.

use crate::{Invocation, Output};
use jails_support::{Failure, Result};

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = invocation.root()?;
    let manifest = crate::model_command::resolve_manifest(None)?;
    let (source, model) = crate::model_command::load_model(&root, &manifest, invocation.output)?;
    let entities = model.entities.len();
    let operations = model.operations.len();
    let declared = format!(
        "{} entit{}, {} operation{}, {} capabilit{}",
        model.entities.len(),
        if model.entities.len() == 1 {
            "y"
        } else {
            "ies"
        },
        model.operations.len(),
        if model.operations.len() == 1 { "" } else { "s" },
        model.capabilities.len(),
        if model.capabilities.len() == 1 {
            "y"
        } else {
            "ies"
        },
    );
    let capabilities = model
        .capabilities
        .values()
        .map(|capability| capability.kind.clone())
        .collect::<Vec<_>>();
    let snapshot = jails_project::capture::capture(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        None,
        &[],
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;

    let name = snapshot.model.model.project.name.clone();
    let facts = &snapshot.project;
    let lock = match snapshot.accepted_model.as_ref() {
        None => Acceptance::Nothing,
        Some(accepted) if accepted == &snapshot.model.model => Acceptance::Accepted,
        Some(_) => Acceptance::Pending,
    };
    let counts = crate::model_ownership::managed_counts(&snapshot);
    // Which capabilities own a service in `compose.yaml`, asked of the file
    // itself rather than of a table: `add db` writes the block under the
    // capability's own marker, so the marker *is* the capability's label and
    // a second list here would be the drift the marked block exists to stop.
    let services = jails_project::compose::read(&root).map_or_else(
        |_| Vec::new(),
        |compose| {
            capabilities
                .iter()
                .filter(|kind| jails_project::compose::declares(&compose, kind))
                .cloned()
                .collect::<Vec<_>>()
        },
    );

    if invocation.output != Output::Human {
        return crate::model_command::print_json(&serde_json::json!({
            "schema": SCHEMA,
            "project": {
                "name": name,
                "platform": snapshot.model.model.project.platform,
                "build": build_word(facts.build_system),
                "java_release": facts.java_release,
            },
            "model": {
                "path": manifest.to_string_lossy(),
                "entities": entities,
                "operations": operations,
                "capabilities": capabilities,
            },
            "lock": lock.word(),
            "managed": {
                "total": counts.total,
                "edited": counts.edited,
                "missing": counts.missing,
            },
            "services": services,
        }));
    }

    println!(
        "{name}  {} / {} / java {}",
        snapshot.model.model.project.platform,
        build_word(facts.build_system),
        facts.java_release
    );
    println!("  model    {declared}  ({})", manifest.display());
    println!("  lock     {}", lock.sentence());
    match counts.total {
        0 => println!("  managed  nothing generated yet"),
        total => {
            let mut line = format!("{total} file{}", if total == 1 { "" } else { "s" });
            if counts.edited > 0 {
                line.push_str(&format!(", {} edited", counts.edited));
            }
            if counts.missing > 0 {
                line.push_str(&format!(
                    ", {} missing -- `jails entity repair`",
                    counts.missing
                ));
            }
            println!("  managed  {line}");
        }
    }
    if !services.is_empty() {
        // Named by the capability that owns the block rather than by the
        // image inside it: `db` is what the reader declared and what `remove
        // db` takes away, and the service under it is the capability's
        // business.
        println!(
            "  compose  services for {} -- `jails start` runs them",
            services.join(", ")
        );
    }
    Ok(())
}

const SCHEMA: &str = "jails.status.v1";

/// The build language, as the reader spells it.
///
/// `BuildSystem::Unknown` is an observation rather than an absence -- jails
/// looked and it was neither -- so it says so rather than printing nothing.
fn build_word(build: jails_model::BuildSystem) -> &'static str {
    match build {
        jails_model::BuildSystem::Maven => "maven",
        jails_model::BuildSystem::Gradle => "gradle",
        jails_model::BuildSystem::Unknown => "no build file",
    }
}

/// What the lock has to say about the model beside it.
#[derive(Clone, Copy)]
enum Acceptance {
    /// No plan has been applied here yet.
    Nothing,
    /// The lock accepted exactly this model.
    Accepted,
    /// The model has moved since the last applied plan.
    Pending,
}

impl Acceptance {
    fn word(self) -> &'static str {
        match self {
            Self::Nothing => "none",
            Self::Accepted => "accepted",
            Self::Pending => "pending",
        }
    }

    fn sentence(self) -> &'static str {
        match self {
            Self::Nothing => "nothing accepted yet -- `jails sync` applies the model",
            Self::Accepted => "accepted",
            Self::Pending => "the model has changes it has not accepted -- `jails sync`",
        }
    }
}

/// The usage a reader outside a project gets, which is the one clap printed.
///
/// **Bare `jails` outside a project is a person who has not started yet**,
/// and the twenty commands are the answer to that. Byte for byte what clap
/// wrote before there was a status to print, on the same stream and with the
/// same exit status, so a script that tested for either still sees it.
pub(crate) fn usage() -> std::process::ExitCode {
    use clap::CommandFactory;
    let mut command = crate::Cli::command();
    // stderr, because that is where clap wrote it when the subcommand was
    // required: a script that reads `jails` output on stdout should not
    // suddenly find a usage page in it.
    let _ = command.write_help(&mut std::io::stderr());
    let _ = std::io::Write::write_all(&mut std::io::stderr(), b"\n");
    std::process::ExitCode::from(2)
}

/// Whether this directory is one bare `jails` has anything to say about.
pub(crate) fn in_a_project(invocation: &Invocation) -> bool {
    invocation
        .root()
        .is_ok_and(|root| crate::model_command::owns(&root))
}
