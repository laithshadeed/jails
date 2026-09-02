//! Apply a small, declarative application manifest.
//!
//! This is deliberately domain-blind. A crawler and a support inbox are two
//! different lists of the same generic intents; neither gets a command,
//! branch, enum, or template in Jails core.
//!
//! The manifest is *replayed* row by row into the model, through the same
//! frontends `jails g` and `jails add` use -- see [`replay`] for why that is
//! not a second editable source, and [`refuse_manifest`] for the one
//! subcommand that refuses on a canonical project.

mod manifest;
use manifest::*;

use crate::ArtifactKind;
use crate::Capability;
use crate::cli::GenerateArgs;
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
    /// Composite unique keys the table carries beside its primary key.
    uniques: Vec<String>,
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
    /// One type rather than a manifest-shaped copy of it: the manifest's own
    /// syntax dies here, where the deprecated `strategy_on`/`strategy_yields`
    /// spellings are resolved by the parser that read them, which is the only
    /// place that should ever know they exist.
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
            .chain(self.uniques.iter())
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
            uniques: self.uniques,
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

/// The recipe name as clap's canonical value, never an alias, or one kind
/// would be keyed under two names.
fn recipe_of(intent: &GenerateArgs) -> String {
    intent
        .kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

/// The recorded name, which is what the duplicate check keys on.
fn recorded_name_of(intent: &GenerateArgs) -> String {
    crate::recorded_name(intent.kind, &intent.name)
}

/// Identity **and** content, as one string.
///
/// Named for what it is, and never used as a key: a key that mixes identity
/// with content cannot answer "is this the same entity?", so a duplicate
/// check on it would accept one entity declared twice with different fields
/// and apply both. Identity is `(recipe_of, recorded_name_of, package)`; this
/// exists only for whole-intent equivalence.
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

/// `app init` *writes* a manifest, which would be a second editable authority
/// beside the model, so a canonical project refuses it.
///
/// **Only `init`.** `plan` and `apply` replay instead -- see [`replay`]. The
/// distinction is between writing a second editable source and reading one
/// once: a replay puts declarations into the model, and the model is what
/// every later command reads. There is deliberately no second implementation
/// of `plan`: two implementations of one question disagree, so `plan` *is*
/// `apply` stopped one step before the lock, and what it names is exactly
/// what the apply then writes.
fn refuse_manifest(command: &str) -> Result<()> {
    Err(jails_support::Failure::Told(format!(
        "`{command}` writes a manifest, and this project already has a model: `.jails/model.jdl` is the one editable source.\n       fix: declare capabilities and generators in the model and run `jails sync`, or replay an existing manifest with `jails app apply`"
    )))
}

pub(crate) fn run(command: AppCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        // **`app init` writes a manifest, which beside a model is a second
        // editable source -- so it refuses on a canonical project and only
        // there.** On a project with no model it is the on-ramp: write the
        // starter manifest, edit it, and `app apply` replays it into the model
        // that first apply creates. Refusing everywhere would leave the
        // manifest format with no way to be written at all.
        AppCommand::Init { manifest } => match crate::model_command::owns() {
            true => refuse_manifest("app init"),
            false => init(manifest.as_deref(), invocation),
        },
        // A plan reports and writes nothing, model included: seeding one here
        // would make `app plan` the command that turns a project canonical.
        AppCommand::Plan { manifest } => {
            crate::model_command::ensure_owned(invocation.clone().pretending())
                .and_then(|()| replay(manifest.as_deref(), invocation.pretending()))
        }
        AppCommand::Apply { manifest, no_start } => {
            // A manifest's capabilities are replayed through `add`, so its
            // compose services are started the same way -- and suppressed the
            // same way.
            let invocation = invocation.without_starting(no_start);
            crate::model_command::ensure_owned(invocation.clone())
                .and_then(|()| replay(manifest.as_deref(), invocation))
        }
    }
}

/// Seed `.jails/app.toml` and hand the file to the reader.
///
/// **One file, whose bytes stop being jails' the moment it lands.** Nothing is
/// recorded about it: what the manifest goes on to declare is `app apply`'s
/// business, and a claim here would be a claim on a document jails does not
/// write again. So it is a one-shot write rather than a transition, for the
/// same reason `adopt` and `modernize` are.
///
/// Seeding is not regeneration, which is why an existing manifest is a refusal
/// rather than a merge. The bytes below are a skeleton nobody keeps; a
/// manifest that exists is a document somebody has been writing, and merging
/// one into the other produces a file neither of them meant.
fn init(manifest: Option<&Path>, invocation: crate::Invocation) -> Result<()> {
    let root = crate::model_command::root()?;
    let target = manifest.map_or_else(|| root.join(DEFAULT_MANIFEST), |path| root.join(path));
    if target.exists() {
        return Err(jails_support::Failure::Told(format!(
            "application manifest already exists: {}.\n       fix: edit it, or pass --manifest with a new path",
            target.display()
        )));
    }
    let skeleton = format!(
        "\
# Generic application intent. Add capabilities, then one [[generate]] table per slice.
schema = {}
capabilities = []

# [[generate]]
# kind = \"scaffold\"
# name = \"Note\"
# fields = [\"id:uuid@pk\", \"title:string!\"]
# timestamps = true
",
        jails_spec::spec::manifest::APP_MANIFEST_SCHEMA
    );
    if invocation.pretend {
        println!("  create  {}", target.display());
        println!("nothing was written.");
        return Ok(());
    }
    jails_support::apply::put_one_shot(&target, skeleton)?;
    println!("  create  {}", target.display());
    println!("Edit it, then run `jails app apply` to replay it into the model.");
    Ok(())
}

/// Replay a manifest into the model, one row at a time.
///
/// **The manifest is an import format, not a second engine.** A row is a
/// `GenerateArgs` -- the same value `jails g` parses -- so every row goes
/// through the frontend that already knows how to declare it, and every
/// capability through `model_capability`. Nothing here decides what a row
/// means; the manifest's own syntax is the only thing this file knows that the
/// CLI does not.
///
/// Two *editable* sources are forbidden. A one-way replay is not one: it
/// writes declarations into the model and the model is what every later
/// command reads.
///
/// **Row by row rather than one transition.** Each frontend is idempotent --
/// a second `g record Order id:uuid` reports `0 files written` -- so an
/// interrupted replay converges by being run again, with no journal to resume
/// from. What it costs is atomicity: a manifest that fails on row nine leaves
/// rows one to eight applied.
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
    // **A plan against a project with no model reports the manifest itself.**
    // Every frontend below needs a model to patch, and seeding one would make
    // `app plan` the command that turns a project canonical. Each row is
    // planned against the model on disk, and under `--pretend` nothing is
    // written -- so row two would plan against a model that does not yet
    // have row one's enum in it and refuse over a type the apply would have
    // declared a moment earlier. What a plan can honestly say here is what
    // applying would declare, which is the question somebody asks before
    // running `apply` the first time.
    if invocation.pretend && !crate::model_command::owns_at(root) {
        println!(
            "  model   {} would be created",
            crate::model_command::JDL_PATH
        );
        for capability in &manifest.capabilities {
            println!("  declare capability {}", capability.label());
        }
        for row in &rows {
            println!(
                "  declare {} {}",
                crate::model_generate::kind_name(row.kind),
                row.name
            );
        }
        println!("nothing was written.");
        return Ok(());
    }
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
    let declared = rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for row in rows {
        crate::model_generate_jdl::run(row, invocation.clone())?;
    }
    report_undeclared(root, &declared, &invocation)
}

/// Name what the model still declares and the manifest no longer does.
///
/// **A manifest is desired state, so a row that left is a statement.** The
/// replay only ever adds -- every frontend it drives is a declaration -- so
/// without this a deleted row is silently nothing, and `app plan` answers "no
/// change" about a manifest that dropped an entire resource.
///
/// It reports rather than removes. Retiring a stored entity needs a storage
/// policy, and choosing one on the reader's behalf is the one thing a
/// declarative replay must not do: `preserve` and `drop` differ by whether the
/// rows still exist afterwards.
fn report_undeclared(
    root: &Path,
    declared: &std::collections::BTreeSet<String>,
    invocation: &crate::Invocation,
) -> Result<()> {
    if invocation.output != crate::Output::Human {
        return Ok(());
    }
    let manifest = crate::model_command::resolve_manifest_at(root, None)?;
    let (source, model) = crate::model_command::load_model_at(root, &manifest, invocation.output)?;
    let _ = source;
    for entity in model.entities.values() {
        if !entity.active || declared.contains(&entity.names.java_type) {
            continue;
        }
        println!(
            "  undeclared {} -- the manifest no longer declares it",
            entity.names.java_type
        );
        // **The plan for removing it, and its refusal is the command's.** A
        // manifest that stops declaring a row is asking for it to go, and a
        // row with a table behind it cannot go without somebody saying what
        // happens to the data. Swallowing that refusal would leave `app
        // apply` exiting 0 over a retirement it had not performed and could
        // not perform -- the reader's next `app apply` would report the same
        // line again, forever. A row with no table plans cleanly and is only
        // reported.
        crate::model_destroy::run(
            crate::model_destroy::Request {
                kind: crate::ArtifactKind::Record,
                name: entity.names.java_type.clone(),
                package: false,
                storage: None,
                confirm_table: None,
                migration_effect: false,
            },
            invocation.clone().pretending(),
        )?;
    }
    Ok(())
}

/// What applying a manifest left behind.
///
/// One variant, kept as a type rather than collapsed to `()` because the
/// *caller* is what makes the distinction real: `jails new --app` publishes
/// by rename, so an error thrown out of the apply discards the whole scratch
/// tree, and an outcome that must be reported without unmaking the project
/// needs a spelling that is not an error. The replay has no post-commit
/// effect to fail, so only the clean outcome exists.
pub(crate) enum Applied {
    /// The replay finished and everything after it succeeded.
    Clean,
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
