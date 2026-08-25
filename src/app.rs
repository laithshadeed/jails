//! Apply a small, declarative application manifest through Jails' existing
//! capability and generator engines.
//!
//! This is deliberately domain-blind. A crawler and a support inbox are two
//! different lists of the same generic intents; neither gets a command,
//! branch, enum, or template in Jails core.

mod manifest;
use manifest::*;

use crate::add::Capability;
use crate::generate::{self, ArtifactKind};
use clap::{Subcommand, ValueEnum};
use jails_engine::route::Intent;
use jails_support::Result;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MANIFEST: &str = ".jails/app.toml";
#[derive(Subcommand)]
pub(crate) enum AppCommand {
    /// Create a documented starter manifest for this project
    Init {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Show the generic capability and generation intents without writing
    Plan {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Apply every unapplied intent, recording progress after each one
    Apply {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write Compose services but do not start them
        #[arg(long)]
        no_start: bool,
    },
}

#[derive(Debug, Default)]
struct Manifest {
    schema: u32,
    capabilities: Vec<Capability>,
}

#[derive(Debug, Default)]
struct GenerateIntent {
    kind: Option<ArtifactKind>,
    name: Option<String>,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
    method: Option<jails_spec::spec::kind::HttpMethod>,
}

impl GenerateIntent {
    /// The row, as the engine takes it.
    ///
    /// One type rather than two. `pending.md` §6.2: a `[[generate]]` row used
    /// to become a `ResolvedIntent` here, which became a `route::Intent` at
    /// the call site, which became an `IntentSpec` inside the route -- three
    /// copies of one request before anything checked it. The manifest's own
    /// syntax is what justified the first of those, and it dies here instead:
    /// the deprecated `strategy_on`/`strategy_yields` spellings are resolved
    /// by the parser that read them, which is the only place that should ever
    /// have known they exist.
    fn finish(self, number: usize) -> Result<Intent> {
        let kind = self
            .kind
            .ok_or_else(|| format!("[[generate]] #{number} is missing `kind`"))?;
        let name = self
            .name
            .ok_or_else(|| format!("[[generate]] #{number} is missing `name`"))?;
        if name.is_empty() {
            return Err(format!("[[generate]] #{number} has an empty `name`").into());
        }
        for value in self
            .fields
            .iter()
            .chain(self.indexes.iter())
            .chain(self.package.iter())
            .chain(self.strategy_on.iter())
            .chain(self.strategy_yields.iter())
        {
            if value.contains(['\n', '\r', '|']) {
                return Err(format!(
                    "[[generate]] #{number} contains a newline or `|`, which is not allowed"
                )
                .into());
            }
        }
        Ok(Intent {
            kind,
            name,
            fields: self.fields,
            timestamps: self.timestamps,
            indexes: self.indexes,
            package: self.package,
            on: self.strategy_on,
            yields: self.strategy_yields,
            method: self.method,
        })
    }
}

/// The recipe name as the ledger spells it -- clap's canonical value, never
/// an alias, or one kind would be stored under two names.
fn recipe_of(intent: &Intent) -> String {
    intent
        .kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

/// The name the ledger row carries, which is what the duplicate check keys on.
fn recorded_name_of(intent: &Intent) -> String {
    generate::recorded_name(intent.kind, &intent.name)
}

/// Identity **and** content, as one string.
///
/// Named for what it is. It was called `key`, and being used as one is the
/// defect plan.md R1 names first: a key that mixes identity with content
/// cannot answer "is this the same entity?", so the manifest's duplicate check
/// accepted one entity declared twice with different fields and applied both,
/// the second overwriting the first's row. Identity is
/// `(recipe_of, recorded_name_of, package)`; this stays only for whole-intent
/// equivalence.
#[cfg(test)]
fn fingerprint(intent: &Intent) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        recipe_of(intent),
        intent.name,
        intent.package.as_deref().unwrap_or(""),
        intent.fields.join(","),
        intent.timestamps,
        intent.indexes.join(","),
        intent.on.as_deref().unwrap_or(""),
        intent.yields.as_deref().unwrap_or(""),
    )
}

/// `app plan` and `app apply` are one route and one flag apart.
///
/// There is deliberately no second implementation of `plan`. V1 answered it
/// with its own walk over the intent list, comparing each row against the
/// ledger and printing `pending`/`update`/`applied` -- a walk that could not
/// see a file the reader had edited and could not tell a regeneration that
/// changes nothing from one that rewrites a class. It had to be shadowed
/// against a typed comparison precisely because two implementations of one
/// question disagree. Here `plan` *is* `apply` stopped one step before the
/// lock, so what it names is exactly what the apply then writes.
pub(crate) fn run(command: AppCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        AppCommand::Init { manifest } => crate::dispatch::mutate(invocation, false, |run| {
            jails_engine::route::app_init(run, manifest.as_deref().and_then(Path::to_str))
        }),
        AppCommand::Plan { manifest } => {
            crate::dispatch::mutate(invocation.pretending(), false, |run| {
                declared(run, manifest.as_deref())
            })
        }
        AppCommand::Apply { manifest, no_start } => {
            crate::dispatch::mutate(invocation, no_start, |run| {
                declared(run, manifest.as_deref())
            })
        }
    }
}

/// The whole manifest, declared as one transition.
///
/// One pass, not two. V1 reconciled every capability a second time because a
/// capability wires itself into what the project has, and a test a *later*
/// row writes was invisible to the row that needed to wire it. That is fixed
/// where it belongs -- the capability writing a `@SpringBootTest` puts the
/// container import in itself -- so a second pass would only run the formatter
/// twice and open a transaction with nothing in it.
fn declared(
    run: &jails_engine::route::Run,
    requested: Option<&Path>,
) -> Result<jails_engine::route::Outcome> {
    let path = manifest_path(run.project().root(), requested)?;
    let (manifest, intents) = read_manifest(&path)?;
    jails_engine::route::app_apply(run, &manifest.capabilities, &intents)
}

/// Apply a manifest against a project root the caller already knows.
///
/// `run` finds the root from the process CWD, which is right for `jails app
/// apply` and wrong for `jails new --app`: the project it should apply to is
/// the one that command just created, not whatever encloses the directory the
/// user is standing in.
pub(crate) fn apply_in(root: &Path, no_start: bool, debug: bool) -> Result<()> {
    let discovering = std::time::Instant::now();
    let project = crate::model::Project::load(root)?;
    let mut run = jails_engine::route::Run::committing(&project).with_timing(
        jails_prepare::timing::TimingPhase::Discover,
        discovering.elapsed(),
    );
    if no_start {
        run = run.without_start();
    }
    if debug {
        run = run.with_debug();
    }
    let outcome = declared(&run, None)?;
    crate::dispatch::report(
        &outcome,
        crate::Output::Human,
        jails_prepare::review::ReviewSelection::default(),
        debug,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `on`/`yields` are the names; the `strategy_*` spellings still parse.
    ///
    /// The alias is not politeness -- `.jails/app.toml` is a file people have
    /// already written, and four manifests in `examples/` use the old keys.
    /// What the alias must not do is let one intent set the same reference
    /// twice under two names, which would make the state key depend on which
    /// line the parser saw last.
    #[test]
    fn on_and_yields_are_the_names_and_the_strategy_spellings_still_parse() {
        let canonical = r#"
                schema = 1

                [[generate]]
                kind = "usecase"
                name = "QueueWork"
                on = "WorkItem"
                yields = "WorkQueued"
        "#;
        let legacy = canonical
            .replace("on = ", "strategy_on = ")
            .replace("yields = ", "strategy_yields = ");

        let (_, new_intents) = parse_manifest(canonical).unwrap();
        let (_, old_intents) = parse_manifest(&legacy).unwrap();
        assert_eq!(new_intents[0].on.as_deref(), Some("WorkItem"));
        assert_eq!(new_intents[0].yields.as_deref(), Some("WorkQueued"));
        // Identical intents, so identical state keys: renaming the key in a
        // manifest must not make `app apply` see a new intent and refuse on
        // files that already exist.
        assert_eq!(fingerprint(&new_intents[0]), fingerprint(&old_intents[0]));
    }

    #[test]
    fn one_reference_may_not_be_set_under_both_spellings() {
        let error = parse_manifest(
            r#"
                schema = 1

                [[generate]]
                kind = "usecase"
                name = "QueueWork"
                on = "WorkItem"
                strategy_on = "SomethingElse"
        "#,
        )
        .unwrap_err();
        assert!(error.contains("deprecated alias"), "{error}");
    }

    #[test]
    fn parses_a_domain_blind_application_manifest() {
        let (_, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["db", "api"]

                [[generate]]
                kind = "enum"
                name = "Status"
                fields = ["PENDING", "DONE"]

                [[generate]]
                kind = "scaffold"
                name = "Task"
                fields = ["id:uuid@pk", "status:Status"]
                indexes = ["status, id"]
            "#,
        )
        .unwrap();
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[1].name, "Task");
        assert_eq!(intents[1].indexes, ["status, id"]);
    }

    #[test]
    fn rejects_unknown_keys_instead_of_silently_ignoring_them() {
        let error = parse_manifest(
            r#"
                schema = 1
                capabilities = []
                [[generate]]
                kind = "record"
                name = "Task"
                feilds = ["id:uuid"]
            "#,
        )
        .unwrap_err();
        assert!(error.contains("feilds"), "{error}");
    }

    #[test]
    fn a_capability_only_application_is_valid() {
        let (manifest, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["api", "actuator"]
            "#,
        )
        .unwrap();
        assert_eq!(manifest.capabilities.len(), 2);
        assert!(intents.is_empty());
    }
}
