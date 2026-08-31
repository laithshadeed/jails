//! Apply a small, declarative application manifest through Jails' existing
//! capability and generator engines.
//!
//! This is deliberately domain-blind. A crawler and a support inbox are two
//! different lists of the same generic intents; neither gets a command,
//! branch, enum, or template in Jails core.

mod manifest;
use manifest::*;

use crate::add::Capability;
use crate::cli::GenerateArgs;
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
    /// The second resource a `query` reads. One spelling only -- it is new,
    /// so there is no shipped alias to keep working.
    via: Option<String>,
    /// A `query`'s declared result order and row ceiling.
    order_by: Option<String>,
    limit: Option<u32>,
    /// The target component whose unique constraint makes this create a
    /// get-or-create.
    on_conflict: Option<String>,
    /// The route a generated endpoint answers.
    path: Option<String>,
    /// Which component identifies the row a `transition` updates.
    select: Option<String>,
    /// Components pinned to a constant, as `component=literal`.
    set: Vec<String>,
    /// Whether a `transition` insists on the caller's `If-Match`.
    if_match: Option<jails_spec::spec::kind::Precondition>,
    /// Components bound from a request parameter of a different name.
    bind: Vec<String>,
    method: Option<jails_spec::spec::kind::HttpMethod>,
    /// How that endpoint reads its request: `json` (the default) or `form`.
    consumes: Option<jails_spec::spec::kind::WireFormat>,
}

impl GenerateIntent {
    /// The row, as `jails g` takes it.
    ///
    /// One type rather than two. `pending.md` §6.2: a `[[generate]]` row used
    /// to become a `ResolvedIntent` here, which became a `route::Intent` at
    /// the call site, which became an `IntentSpec` inside the route -- three
    /// copies of one request before anything checked it. The manifest's own
    /// syntax is what justified the first of those, and it dies here instead:
    /// the deprecated `strategy_on`/`strategy_yields` spellings are resolved
    /// by the parser that read them, which is the only place that should ever
    /// have known they exist.
    fn finish(self, number: usize) -> Result<GenerateArgs> {
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
            .chain(self.via.iter())
            .chain(self.order_by.iter())
            .chain(self.on_conflict.iter())
            .chain(self.path.iter())
        {
            if value.contains(['\n', '\r', '|']) {
                return Err(format!(
                    "[[generate]] #{number} contains a newline or `|`, which is not allowed"
                )
                .into());
            }
        }
        Ok(GenerateArgs {
            kind,
            name,
            fields: self.fields,
            timestamps: self.timestamps,
            indexes: self.indexes,
            package: self.package,
            strategy_on: self.strategy_on,
            strategy_yields: self.strategy_yields,
            // Field evolution only, and a manifest declares no evolution:
            // every row is a thing to exist, not a change to one that does.
            default_literal: None,
            backfill_file: None,
            via: self.via,
            order_by: self.order_by,
            limit: self.limit,
            on_conflict: self.on_conflict,
            path: self.path,
            select: self.select,
            set: self.set,
            if_match: self.if_match,
            bind: self.bind,
            method: self.method,
            consumes: self.consumes,
        })
    }
}

/// The recipe name as the ledger spells it -- clap's canonical value, never
/// an alias, or one kind would be stored under two names.
fn recipe_of(intent: &GenerateArgs) -> String {
    intent
        .kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

/// The name the ledger row carries, which is what the duplicate check keys on.
fn recorded_name_of(intent: &GenerateArgs) -> String {
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
fn fingerprint(intent: &GenerateArgs) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        recipe_of(intent),
        intent.name,
        intent.package.as_deref().unwrap_or(""),
        intent.fields.join(","),
        intent.timestamps,
        intent.indexes.join(","),
        intent.strategy_on.as_deref().unwrap_or(""),
        intent.strategy_yields.as_deref().unwrap_or(""),
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
/// The manifest is a second desired-state authority, so a canonical project
/// refuses it.
///
/// Every other mutating command was gated at the `main.rs` match and this one
/// was not, which made it the one way to reach the legacy engine from a
/// project that owns `.jails/model.jdl`: `app apply` planned `jails.toml`, a
/// legacy ledger and capability Java into `src/main/java`, outside the managed
/// tree, against a model it had never read. Two editable authorities writing
/// one project is the disease this compiler exists to cure, and a *planner*
/// that can still be invoked is not a cured one.
///
/// All three subcommands refuse, not only `apply`. `init` writes the second
/// authority, and `plan` renders a legacy transition the canonical executor
/// would never perform -- a preview of work that cannot happen is worse than
/// a refusal, because the reader believes it.
fn refuse_manifest(command: &str) -> Result<()> {
    crate::model_command::refuse_legacy_mutation(
        command,
        "declare capabilities and generators in the canonical model and run `jails sync`; `.jails/app.toml` is the legacy manifest and is not a second source",
    )
}

pub(crate) fn run(command: AppCommand, invocation: crate::Invocation) -> Result<()> {
    if crate::model_command::owns() {
        return match command {
            // `app init` writes a manifest, which would be a second editable
            // source beside the model. That one still refuses.
            AppCommand::Init { .. } => refuse_manifest("app init"),
            AppCommand::Plan { manifest } => replay(manifest.as_deref(), invocation.pretending()),
            AppCommand::Apply { manifest, .. } => replay(manifest.as_deref(), invocation),
        };
    }
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

/// Replay a manifest into the model, one row at a time.
///
/// **The manifest becomes an import format rather than a second engine**, and
/// that is the difference between this and the refusal it replaces. A row is a
/// `GenerateArgs` -- the same value `jails g` parses -- so every row goes
/// through the frontend that already knows how to declare it, and every
/// capability through `model_capability`. Nothing here decides what a row
/// means; the manifest's own syntax is the only thing this file knows that the
/// CLI does not.
///
/// Refusing was defensible while the alternative was a parallel engine: two
/// *editable* sources is what the cutover forbids. A one-way replay is not
/// one, for the same reason `model import` is not -- it writes declarations
/// into the model and the model is what every later command reads.
///
/// **Row by row rather than one transition, and that is an improvement.**
/// Each frontend is idempotent -- a second `g record Order id:uuid` reports
/// `0 files written` -- so an interrupted replay converges by being run again,
/// where the legacy path needed a journal to resume from. What it costs is
/// atomicity: a manifest that fails on row nine leaves rows one to eight
/// applied. The legacy engine's own answer to that was the journal, and a
/// canonical project has no journal by design.
fn replay(requested: Option<&Path>, invocation: crate::Invocation) -> Result<()> {
    let root = invocation.root()?;
    replay_at(&root, requested, invocation)
}

/// The same replay, against a project the caller has resolved.
///
/// `jails new --app` is the caller: it stands in the parent of the project it
/// is creating, so `model_command::root` would walk to the wrong one.
pub(crate) fn replay_at(
    root: &Path,
    requested: Option<&Path>,
    invocation: crate::Invocation,
) -> Result<()> {
    let invocation = invocation.at(root.to_path_buf());
    let path = manifest_path(root, requested)?;
    let (manifest, rows) = read_manifest(&path)?;
    // Every capability in one patch, then every row. The manifest already
    // parsed its capability list into the closed vocabulary, so there is no
    // second lookup to get wrong here.
    if !manifest.capabilities.is_empty() {
        crate::model_capability::add(
            manifest.capabilities.clone(),
            None,
            None,
            invocation.clone(),
        )?;
    }
    for row in rows {
        crate::model_generate_jdl::run(row, invocation.clone())?;
    }
    Ok(())
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
    let (manifest, rows) = read_manifest(&path)?;
    // The rows are `GenerateArgs`, the same value `jails g` parses, so the
    // canonical path can replay a manifest through the ordinary frontends.
    // The legacy engine takes its own shape, and the conversion that already
    // existed for the CLI does the work.
    let intents: Vec<Intent> = rows.into_iter().map(Into::into).collect();
    jails_engine::route::app_apply(run, &manifest.capabilities, &intents)
}

/// Apply a manifest against a project root the caller already knows.
///
/// `run` finds the root from the process CWD, which is right for `jails app
/// apply` and wrong for `jails new --app`: the project it should apply to is
/// the one that command just created, not whatever encloses the directory the
/// user is standing in.
/// What applying a manifest left behind.
///
/// Two outcomes rather than one `Result`, because `jails new --app` has to
/// treat them differently: a manifest that could not be applied leaves no
/// project, and a manifest that *was* applied leaves one whether or not a
/// post-commit effect succeeded.
pub(crate) enum Applied {
    /// The transaction committed and everything after it succeeded.
    Clean,
    /// The transaction committed; something after it failed, and the reason
    /// has already been printed.
    CommittedThenReported,
}

pub(crate) fn apply_in(root: &Path, no_start: bool, debug: bool) -> Result<Applied> {
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
    let recovered = jails_engine::route::finish_interrupted(&project)?;
    let outcome = declared(&run, None)?.after_recovery(recovered);
    let committed = outcome.is_committed();
    match crate::dispatch::report(
        &outcome,
        &["app".to_string(), "apply".to_string()],
        crate::Output::Human,
        jails_prepare::review::ReviewSelection::default(),
        debug,
    ) {
        Ok(()) => Ok(Applied::Clean),
        Err(error) if committed => {
            // Printed already. Returning it as a value rather than an error
            // is what lets the caller finish publishing the project the
            // commit is in before it reports the failure.
            let _ = error;
            Ok(Applied::CommittedThenReported)
        }
        Err(error) => Err(error),
    }
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
