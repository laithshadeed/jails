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
}

impl GenerateIntent {
    fn finish(self, number: usize) -> Result<ResolvedIntent> {
        let kind = self
            .kind
            .ok_or_else(|| format!("[[generate]] #{number} is missing `kind`"))?;
        let name = self
            .name
            .ok_or_else(|| format!("[[generate]] #{number} is missing `name`"))?;
        if name.is_empty() {
            return Err(format!("[[generate]] #{number} has an empty `name`"));
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
                ));
            }
        }
        Ok(ResolvedIntent {
            kind,
            name,
            fields: self.fields,
            timestamps: self.timestamps,
            indexes: self.indexes,
            package: self.package,
            strategy_on: self.strategy_on,
            strategy_yields: self.strategy_yields,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedIntent {
    kind: ArtifactKind,
    name: String,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
}

impl ResolvedIntent {
    /// The same row, as the engine takes it.
    ///
    /// Two types rather than one on purpose: this one carries manifest syntax
    /// -- the deprecated `strategy_on`/`strategy_yields` spellings, and the
    /// `timestamps` flag that is expanded before any recipe sees it -- and the
    /// engine has no business knowing about a file format.
    fn declared(&self) -> jails_engine::route::Intent {
        jails_engine::route::Intent {
            kind: self.kind,
            name: self.name.clone(),
            fields: self.fields.clone(),
            timestamps: self.timestamps,
            indexes: self.indexes.clone(),
            package: self.package.clone(),
            on: self.strategy_on.clone(),
            yields: self.strategy_yields.clone(),
        }
    }

    #[cfg(test)]
    /// Identity **and** content, as one string.
    ///
    /// Named for what it is. It was called `key`, and being used as one is the
    /// defect plan.md R1 names first: a key that mixes identity with content
    /// cannot answer "is this the same entity?", so the manifest's duplicate
    /// check accepted one entity declared twice with different fields and
    /// applied both, the second overwriting the first's row. Identity alone is
    /// [`ResolvedIntent::key`]; this stays only for whole-intent equivalence.
    fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name(),
            self.name,
            self.package.as_deref().unwrap_or(""),
            self.fields.join(","),
            self.timestamps,
            self.indexes.join(","),
            self.strategy_on.as_deref().unwrap_or(""),
            self.strategy_yields.as_deref().unwrap_or("")
        )
    }

    /// The recipe name as the ledger spells it -- clap's canonical value, never
    /// an alias, or one kind would be stored under two names.
    fn recipe(&self) -> String {
        self.kind
            .to_possible_value()
            .expect("every ArtifactKind has a clap value")
            .get_name()
            .to_string()
    }

    /// The name the ledger row carries. See `key`.
    fn recorded_name(&self) -> String {
        generate::recorded_name(self.kind, &self.name)
    }
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
        AppCommand::Init { manifest } => crate::invoke::mutate(invocation, false, |run| {
            jails_engine::route::app_init(run, manifest.as_deref().and_then(Path::to_str))
        }),
        AppCommand::Plan { manifest } => {
            crate::invoke::mutate(invocation.pretending(), false, |run| {
                declared(run, manifest.as_deref())
            })
        }
        AppCommand::Apply { manifest, no_start } => {
            crate::invoke::mutate(invocation, no_start, |run| {
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
    let intents: Vec<jails_engine::route::Intent> =
        intents.iter().map(ResolvedIntent::declared).collect();
    jails_engine::route::app_apply(run, &manifest.capabilities, &intents)
}

/// Apply a manifest against a project root the caller already knows.
///
/// `run` finds the root from the process CWD, which is right for `jails app
/// apply` and wrong for `jails new --app`: the project it should apply to is
/// the one that command just created, not whatever encloses the directory the
/// user is standing in.
pub(crate) fn apply_in(root: &Path, no_start: bool, debug: bool) -> Result<()> {
    let project = crate::model::Project::load(root)?;
    let mut run = jails_engine::route::Run::committing(&project);
    if no_start {
        run = run.without_start();
    }
    if debug {
        run = run.with_debug();
    }
    let outcome = declared(&run, None)?;
    crate::invoke::report(&outcome, crate::Output::Human)
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
        assert_eq!(new_intents[0].strategy_on.as_deref(), Some("WorkItem"));
        assert_eq!(
            new_intents[0].strategy_yields.as_deref(),
            Some("WorkQueued")
        );
        // Identical intents, so identical state keys: renaming the key in a
        // manifest must not make `app apply` see a new intent and refuse on
        // files that already exist.
        assert_eq!(new_intents[0].fingerprint(), old_intents[0].fingerprint());
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
