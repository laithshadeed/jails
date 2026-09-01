//! Compatibility lowering from familiar generation syntax into `ModelPatch`.

#[path = "model_field_parse.rs"]
mod field_parse;
pub(crate) use field_parse::{normalize_type, parse_field};

mod render;

mod effects;
mod report;

pub(crate) use effects::run_follow_up_effects;
use report::refuse_unconfirmed_deletions;
pub(crate) use report::{report_plan, write_bundle};

use render::operation_declaration;
pub(crate) use render::{entity_declaration, enum_declaration, field_declaration};

use crate::ArtifactKind;
use crate::cli::GenerateArgs;
use crate::model_resource::java_to_label;
use crate::{Invocation, Output};
use jails_contracts::{CanonicalModelPatch, ModelFileUpdate, ProjectPath};
use jails_model::{AppModel, EntityId, Facet, ModelPatch, OperationId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

const MODEL_PATH: &str = ".jails/model.toml";

pub(crate) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    if crate::model_command::owns_jdl() {
        return crate::model_generate_jdl::run(args, invocation);
    }
    let native = crate::canonical_support::generator(args.kind).is_native();
    if !native {
        return Err(Failure::Told(format!(
            "canonical model projects do not route `{}` through the legacy generator.\n       fix: use an implemented semantic frontend or add the declaration to {MODEL_PATH}",
            kind_name(args.kind)
        )));
    }
    if args.kind == ArtifactKind::Field {
        return crate::model_resource::add_generated_field(args, invocation);
    }
    if let Some(profile) = entity_profile(args.kind) {
        return run_entity(args, profile, invocation);
    }
    if let Some(profile) = operation_profile(args.kind) {
        return run_operation(args, profile, invocation);
    }
    Err(Failure::Told(format!(
        "canonical `{}` is implemented by the JDL frontend, not the temporary TOML compatibility editor.\n       fix: move the model to `.jails/model.jdl`, or add the declaration directly to {MODEL_PATH}",
        kind_name(args.kind)
    )))
}

fn run_entity(
    args: GenerateArgs,
    profile: &'static EntityProfile,
    invocation: Invocation,
) -> Result<()> {
    reject_unsupported_options(&args, profile)?;
    if !args.uniques.is_empty() {
        return Err(Failure::Told(
            "a composite unique key needs a `jdl 1` model.\n       fix: run `jails model upgrade` and repeat the command"
                .to_string(),
        ));
    }
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = jails_model::parse_toml(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let entity_label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{entity_label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let mut fields = args.fields.clone();
    if args.timestamps {
        fields.extend([
            "createdAt:instant".to_string(),
            "updatedAt:instant".to_string(),
        ]);
    }
    let declaration = if args.kind == ArtifactKind::Enum {
        enum_declaration(&entity_label, &args.name, &fields)?
    } else {
        entity_declaration(&entity_label, &args.name, profile.facets, &fields)?
    };
    let mut next_source = current_source.clone();
    if !next_source.ends_with('\n') {
        next_source.push('\n');
    }
    next_source.push('\n');
    next_source.push_str(&declaration);
    let next_model = jails_model::parse_toml(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let entity = next_model
        .entity(&entity_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new entity `{entity_id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-entity",
        "entity": entity,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddEntity(entity),
        patch_bytes,
        authored_migration: None,
    })
}

fn run_operation(
    args: GenerateArgs,
    profile: OperationProfile,
    invocation: Invocation,
) -> Result<()> {
    reject_unsupported_operation_options(&args, profile)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = jails_model::parse_toml(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let label = java_to_label(&args.name);
    let operation_id = OperationId::parse(format!("op_{label}"))
        .map_err(|error| Failure::Told(format!("could not assign operation identity: {error}")))?;
    let declaration = operation_declaration(&args, profile, &current_model, &label)?;
    let mut next_source = current_source.clone();
    if !next_source.ends_with('\n') {
        next_source.push('\n');
    }
    next_source.push('\n');
    next_source.push_str(&declaration);
    let next_model = jails_model::parse_toml(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let operation = next_model
        .operations
        .get(&operation_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new operation `{operation_id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-operation",
        "operation": operation,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddOperation(operation),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) struct PreparedMutation {
    pub(crate) name: String,
    pub(crate) invocation: Invocation,
    pub(crate) model_path: PathBuf,
    pub(crate) current_source: String,
    pub(crate) current_model: AppModel,
    pub(crate) next_source: String,
    pub(crate) patch: ModelPatch,
    pub(crate) patch_bytes: Vec<u8>,
    /// A migration the *reader* authored, rather than one the compiler
    /// derived from a schema change.
    ///
    /// `jdl-sol.md` §2.1 is explicit that ordered migration files are not JDL
    /// -- "immutable, append-only history" -- and §2 lists writing one among
    /// the *non-model* actions a familiar command may map to. So this is not
    /// smuggled into rendering: it joins `PlanDraft.migrations` beside the
    /// derived ones, and the materializer turns it into an ordinary
    /// `AppendMigration` operation with an allocated version and a `Missing`
    /// precondition. It is as visible in the reviewed plan as any other.
    pub(crate) authored_migration: Option<jails_contracts::RenderedMigration>,
}

/// Report a declaration that was already there.
///
/// Every canonical frontend is idempotent, and the ordinary path says so with
/// `0 files written`. A frontend that can tell *before* preparing a patch --
/// `g association`, where re-issuing `AddRelation` fails on the id rather than
/// reconciling -- returns early instead, and says the same thing from here so
/// the sentence lives with the rest of this module's output.
pub(crate) fn report_already_declared(name: &str) {
    println!("{name} is already declared (0 files written)");
}

/// Where the wall clock went, under `--debug`.
///
/// **Named for the canonical pipeline, because that is what runs.** The legacy
/// engine's phases were `discover / observe / parse / project / prepare /
/// verify`; the compiler's are the five contracts -- capture the workspace,
/// apply the patch to the model, compile it, materialize the exact plan, and
/// execute it. A timing list naming steps the binary no longer has is worse
/// than none, because it sends the reader looking for the wrong thing.
///
/// `execute` is absent on a preview, and that absence is the point: it is how
/// a reader confirms `--pretend` stopped before the only step that writes.
#[derive(Default)]
struct Stopwatch {
    phases: Vec<(&'static str, std::time::Duration)>,
    since: Option<std::time::Instant>,
}

impl Stopwatch {
    fn start(enabled: bool) -> Self {
        Self {
            phases: Vec::new(),
            since: enabled.then(std::time::Instant::now),
        }
    }

    fn mark(&mut self, phase: &'static str) {
        if let Some(since) = self.since {
            self.phases.push((phase, since.elapsed()));
            self.since = Some(std::time::Instant::now());
        }
    }

    fn report(&self) {
        for (phase, elapsed) in &self.phases {
            println!("  timing  {phase:<12}{:>8.1?}", elapsed);
        }
    }
}

pub(crate) fn finish_generation(prepared: PreparedMutation) -> Result<()> {
    finish_generation_with_reader_paths(prepared, &[])
}

pub(crate) fn finish_generation_with_reader_paths(
    prepared: PreparedMutation,
    reader_paths: &[ProjectPath],
) -> Result<()> {
    let PreparedMutation {
        name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
        authored_migration,
    } = prepared;
    report::refuse_legacy_envelope(&invocation)?;
    let mut clock = Stopwatch::start(invocation.debug);
    let canonical_model_path = model_path.to_string_lossy().replace('\\', "/");
    // The invocation's, so `jails new --app` can replay a manifest into the
    // project it is creating rather than into whatever encloses the directory
    // the reader is standing in.
    let root = invocation.root()?;
    let mut next_model = current_model.clone();
    next_model
        .apply(patch.clone())
        .map_err(|error| Failure::Told(format!("could not prepare model capture: {error}")))?;
    let mut capture_paths = reader_paths.to_vec();
    capture_paths.extend(jails_compiler::external_project_paths(&next_model));
    capture_paths.sort();
    capture_paths.dedup();
    let snapshot = jails_workspace::capture_planned(
        &root,
        &model_path,
        current_source.as_bytes(),
        current_model,
        &next_model,
        &capture_paths,
    )
    .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    clock.mark("capture");
    // **The refusal says the whole request was abandoned.** A command naming
    // several things -- `jails add csv security` -- plans all of them and
    // applies all of them or none, and a reader who is not told that has to
    // work out by hand which half to retry. Nothing has been written at this
    // point by construction: the executor has not run.
    let mut draft = jails_compiler::Compiler::compile(&snapshot, Some(patch)).map_err(|error| {
        Failure::Told(format!(
            "could not compile model patch: {error}\n       nothing was written"
        ))
    })?;
    // After the compile, because it is not derived from the model: see
    // `PreparedMutation::authored_migration`. It is still the plan's, not a
    // side effect -- the materializer allocates its version from the observed
    // history and refuses if the path it lands on already exists.
    clock.mark("compile");
    draft.migrations.extend(authored_migration);
    // **What the compiler noticed but would not refuse over.** A warning that
    // stays inside the draft is a warning nobody reads; these are the shapes
    // that compile and run and are probably not what the reader meant, so
    // they belong on the way past rather than in a report they would have to
    // know to ask for.
    if invocation.output == Output::Human {
        for diagnostic in &draft.diagnostics {
            eprintln!("jails: {}", diagnostic.message);
            eprintln!("       fix: {}", diagnostic.fix);
        }
    }
    let bundle = jails_workspace::materialize_with_model(
        &snapshot,
        CanonicalModelPatch {
            schema: "jails.model-patch.v1".to_string(),
            bytes: patch_bytes,
        },
        draft,
        Some(ModelFileUpdate {
            path: ProjectPath::parse(canonical_model_path).map_err(Failure::Told)?,
            bytes: next_source.into_bytes(),
        }),
        jails_compiler::COMPILER_VERSION,
        if invocation.force {
            jails_workspace::Restore::EditedAndRemoved
        } else {
            jails_workspace::Restore::Refuse
        },
    )
    .map_err(|error| Failure::Told(format!("could not materialize exact plan: {error}")))?;
    clock.mark("materialize");

    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        report_plan(&bundle, &invocation)?;
        if invocation.output == Output::Human {
            clock.report();
        }
        return Ok(());
    }
    if let Some(refusal) = refuse_unconfirmed_deletions(&bundle, &invocation) {
        return refusal;
    }
    let stranded = report::stranded_reader_references(&root, &snapshot.model.model, &next_model);
    // **Said only once the model exists, and only if it does.** The on-ramp
    // used to be a transition of its own that ran before the mutation, so a
    // refused command announced a conversion it had then abandoned. Reading
    // the plan for a model file with no before-image says the same thing at
    // the one moment it is true. It goes to stderr because stdout is the
    // command's own output and a caller piping it did not ask for this.
    let converted = invocation.output == Output::Human
        && bundle.plan.operations.iter().any(|operation| {
            matches!(
                operation,
                jails_contracts::PlannedOperation::ReplaceModelFile { before: None, .. }
            )
        });
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply exact plan: {error}")))?;
    clock.mark("execute");
    if converted {
        eprintln!("  create  {}", crate::model_command::JDL_PATH);
        eprintln!(
            "This project is canonical now: `jails g` renders through the compiler into \
             `.jails/generated`, and your own sources under `src/` stay yours."
        );
    }
    if invocation.output == Output::Human {
        for line in &stranded {
            eprintln!("{line}");
        }
        // **"Nothing happened" and "everything happened and changed nothing"
        // are different answers**, and only the second has files to name. A
        // second `jails add csv` is the first, and a reader who cannot tell
        // them apart cannot tell a no-op from a command that silently did not
        // run.
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("{name}: nothing to do, the project already matches the model");
        } else {
            println!(
                "applied model patch for {}: {} ({} files written)",
                name,
                execution.plan_digest.as_str(),
                execution.files_written
            );
            // **Every path, because a count is not an answer.** `g field
            // Order memo:string?` rewrites the query, the transition and the
            // use case that construct `Order`, and reporting "17 files
            // written" leaves a reader who wanted to know whether their
            // companions moved with no way to find out but `git status`. The
            // same lines `--pretend` prints, so the preview and the report
            // cannot describe the transition differently.
            //
            // A deleted file is the one that most needs saying -- it leaves
            // nothing behind, and with `--force` it may have carried an
            // afternoon of edits -- and a migration is the other: it is
            // append-only, so the moment to read it is before it reaches a
            // database.
            for line in crate::model_command::preview_lines(&bundle) {
                println!("{line}");
            }
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution)
                .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?
        );
    }
    report::report_review(&bundle, &invocation);
    if invocation.output == Output::Human {
        clock.report();
    }
    run_follow_up_effects(&root, &bundle, &invocation)
}

const RECORD_FACETS: &[Facet] = &[Facet::Record];
const ENUM_FACETS: &[Facet] = &[Facet::Enum];
const SCAFFOLD_FACETS: &[Facet] = &[
    Facet::Record,
    Facet::Repository,
    Facet::Service,
    Facet::Http,
];

struct EntityProfile {
    facets: &'static [Facet],
    timestamps: bool,
    /// Whether this profile puts a table behind the entity.
    ///
    /// Only `scaffold` does, which is what makes `--unique` meaningful: a
    /// composite unique is a constraint on columns, and a profile with no
    /// columns has nowhere to put one.
    table: bool,
    /// Whether `--path` pins this profile's collection route.
    ///
    /// Only `scaffold` has one: it is the profile that carries `Facet::Http`,
    /// and a route on a kind that serves nothing would be a flag with nowhere
    /// to land.
    route: bool,
}

fn entity_profile(kind: ArtifactKind) -> Option<&'static EntityProfile> {
    static RECORD: EntityProfile = EntityProfile {
        facets: RECORD_FACETS,
        timestamps: false,
        table: false,
        route: false,
    };
    static ENUM: EntityProfile = EntityProfile {
        facets: ENUM_FACETS,
        timestamps: false,
        table: false,
        route: false,
    };
    static SCAFFOLD: EntityProfile = EntityProfile {
        facets: SCAFFOLD_FACETS,
        timestamps: true,
        table: true,
        route: true,
    };
    match kind {
        ArtifactKind::Record | ArtifactKind::Value => Some(&RECORD),
        ArtifactKind::Enum => Some(&ENUM),
        ArtifactKind::Scaffold => Some(&SCAFFOLD),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationProfile {
    Command,
    Query,
    Transition,
    Event,
}

pub(crate) fn operation_profile(kind: ArtifactKind) -> Option<OperationProfile> {
    match kind {
        ArtifactKind::Usecase => Some(OperationProfile::Command),
        ArtifactKind::Query => Some(OperationProfile::Query),
        ArtifactKind::Transition => Some(OperationProfile::Transition),
        ArtifactKind::Event => Some(OperationProfile::Event),
        _ => None,
    }
}

/// Which of the supplied flags this profile has nowhere to put.
///
/// **Named, one at a time, with what it does apply to.** "does not represent
/// one or more supplied flags" is true of every one of these and useful for
/// none: a reader who typed `g record Thing --path /thing` needs to be told
/// that `--path` is for the kinds that answer a route, not to re-read the
/// command looking for which flag was the wrong one.
fn reject_unsupported_options(args: &GenerateArgs, profile: &EntityProfile) -> Result<()> {
    let unsupported: &[(bool, &str, &str)] = &[
        (
            args.timestamps && !profile.timestamps,
            "--timestamps",
            "the kinds that own a table",
        ),
        (
            !args.uniques.is_empty() && !profile.table,
            "--unique",
            "the kinds that own a table",
        ),
        (args.package.is_some(), "--package", "capabilities"),
        (
            args.default_literal.is_some(),
            "--default-literal",
            "`resource field` commands",
        ),
        (
            args.backfill_file.is_some(),
            "--backfill-file",
            "`resource field nullability`",
        ),
        (args.strategy_on.is_some(), "--on", "operations"),
        (args.strategy_yields.is_some(), "--yields", "operations"),
        (args.via.is_some(), "--via", "a query or a use case"),
        (args.order_by.is_some(), "--order-by", "queries"),
        (args.limit.is_some(), "--limit", "queries"),
        (args.on_conflict.is_some(), "--on-conflict", "use cases"),
        (
            args.path.is_some() && !profile.route,
            "--path",
            "the kinds that answer a route",
        ),
        (args.select.is_some(), "--select", "transitions"),
        (!args.set.is_empty(), "--set", "a use case or a transition"),
        (args.if_match.is_some(), "--if-match", "transitions"),
        (!args.bind.is_empty(), "--bind", "a controller"),
        (
            args.method.is_some(),
            "--method",
            "the kinds that answer a route",
        ),
        (
            args.consumes.is_some(),
            "--consumes",
            "the kinds that answer a route",
        ),
    ];
    if let Some((_, flag, applies)) = unsupported.iter().find(|(supplied, _, _)| *supplied) {
        return Err(Failure::Told(format!(
            "`{flag}` applies to {applies}, and `{}` is not one of them.\n       fix: drop `{flag}`, or generate a kind that carries it",
            kind_name(args.kind)
        )));
    }
    Ok(())
}

pub(crate) fn validate_entity_args(args: &GenerateArgs) -> Result<()> {
    let profile = entity_profile(args.kind).ok_or_else(|| {
        Failure::Told(format!(
            "`{}` is not an entity declaration",
            kind_name(args.kind)
        ))
    })?;
    // **Checked before the identity is derived from it.** A stable id is a
    // projection of the name, so `Bad!Name` fails there first -- and reports
    // that `ent_bad!_name` is not a stable id, which is about a value the
    // reader never typed. The linker says the same thing about the Java type
    // and never gets the chance.
    if let Some(bad) = args
        .name
        .chars()
        .find(|character| !character.is_ascii_alphanumeric() && *character != '_')
    {
        return Err(Failure::Told(format!(
            "`{bad}` is not valid in a Java identifier, and `{}` becomes one.\n       fix: name it with letters, digits and `_`",
            args.name
        )));
    }
    reject_unsupported_options(args, profile)
}

pub(crate) fn reject_unsupported_operation_options(
    args: &GenerateArgs,
    profile: OperationProfile,
) -> Result<()> {
    // **Name the flag, and say which kind it belongs to.** "does not
    // represent one or more supplied flags" is true of every one of these and
    // useful for none: the reader has to guess which of the eight they typed
    // is the problem, and the answer is usually that the flag belongs to a
    // sibling kind. One row per flag, so the refusal reads like the sentence
    // somebody would say out loud.
    let kind = kind_name(args.kind);
    let entity_only = "an entity declaration";
    let unsupported: &[(bool, &str, &str)] = &[
        (args.timestamps, "--timestamps", entity_only),
        (args.package.is_some(), "--package", entity_only),
        (args.default_literal.is_some(), "--default", entity_only),
        (args.backfill_file.is_some(), "--backfill", entity_only),
        (!args.indexes.is_empty(), "--index", entity_only),
        (!args.uniques.is_empty(), "--unique", entity_only),
        (
            args.on_conflict.is_some() && profile != OperationProfile::Command,
            "--on-conflict",
            "a command",
        ),
        (
            args.via.is_some()
                && !matches!(profile, OperationProfile::Query | OperationProfile::Command),
            "--via",
            "a query or a command",
        ),
        (
            args.select.is_some() && profile != OperationProfile::Transition,
            "--select",
            "a transition",
        ),
        (
            !args.set.is_empty()
                && !matches!(
                    profile,
                    OperationProfile::Transition | OperationProfile::Command
                ),
            "--set",
            "a transition or a command",
        ),
        (
            args.if_match.is_some() && profile != OperationProfile::Transition,
            "--if-match",
            "a transition",
        ),
        (
            args.consumes.is_some() && profile == OperationProfile::Event,
            "--consumes",
            "an operation with a request boundary",
        ),
        (
            args.order_by.is_some() && profile != OperationProfile::Query,
            "--order-by",
            "a query",
        ),
        (
            args.limit.is_some() && profile != OperationProfile::Query,
            "--limit",
            "a query",
        ),
        (
            args.strategy_yields.is_some()
                && !matches!(
                    profile,
                    OperationProfile::Transition | OperationProfile::Command
                ),
            "--yields",
            "a transition or a command",
        ),
        // **A query's and a command's verb follows its request**, so
        // `--method` there is not a preference jails declines to honour -- it
        // is a claim about the request that contradicts the request.
        (
            args.method.is_some() && profile != OperationProfile::Transition,
            "--method",
            "a transition; every other operation's verb follows its request",
        ),
        (
            args.path.is_some() && profile == OperationProfile::Event,
            "--path",
            "an operation with a route",
        ),
    ];
    if let Some((_, flag, applies_to)) = unsupported.iter().find(|(hit, _, _)| *hit) {
        return Err(Failure::Told(format!(
            "`{flag}` applies to {applies_to}, and `{kind}` is not one.\n       fix: drop `{flag}`, or generate the kind it belongs to"
        )));
    }
    // **An event may stand on its own, and the grammar has always said so.**
    // `parse_operation(None)` accepts a top-level `event`, the linker gives it
    // `on: None`, and the compiler emits its payload record from the declared
    // parameters -- so a domain event that is nobody's row (`PageDiscovered`,
    // carrying its own id and the moment it happened) was refused only by this
    // frontend. Every other operation writes or reads a row and needs one.
    if args.strategy_on.is_none() && profile != OperationProfile::Event {
        return Err(Failure::Told(format!(
            "canonical `{}` needs the entity it operates on.\n       fix: pass `--on <Entity>`",
            kind_name(args.kind)
        )));
    }
    Ok(())
}

pub(crate) fn operation_field_labels(
    model: &AppModel,
    entity: &str,
    fields: &[String],
) -> Result<Vec<String>> {
    operation_field_labels_via(model, entity, None, false, fields)
}

/// The same resolution, with a joined entity's components in scope.
///
/// **A `--via` query's filter may name a component the target does not have**,
/// and that is what the flag is for: `Message` has no `email`, so a query on
/// it was reachable only by a caller that already knew the surrogate user id.
/// `--via User` reads `users` alongside `messages`, and `email` is a column of
/// the join rather than of the row. The model says so directly -- an operation
/// parameter "may name a join alias that has no column on this table".
///
/// Target first, so a name both entities declare resolves against the one the
/// query is on, which is where a reader would expect it to.
pub(crate) fn operation_field_labels_via(
    model: &AppModel,
    entity: &str,
    joined: Option<&str>,
    optional_filters: bool,
    fields: &[String],
) -> Result<Vec<String>> {
    fields
        .iter()
        .map(|field| {
            // **A trailing `?` on a query filter is not a nullable column.**
            // `direction:MessageDirection?` means "filter by direction, or
            // do not" -- three independent optional filters is eight queries
            // written by hand -- and the model has carried `optional_filter`
            // for it all along. Compared against the entity's own optionality
            // it read as a disagreement, so the query refused.
            match optional_filters && field.ends_with('?') {
                true => Ok(format!(
                    "{}?",
                    resolve_filter(model, entity, joined, field.trim_end_matches('?'))?
                )),
                false => resolve(model, entity, joined, field),
            }
        })
        .collect()
}

/// The same, for a filter the caller may omit.
///
/// **The `?` stands in for the entity's own suffix rather than beside it.**
/// `status:string?` on an entity whose `status` is `string!` is the natural
/// spelling of "filter by status, or do not" -- nobody writes `status:string!?`
/// -- so the optionality is what the marker replaced and comparing it against
/// the column's read as a disagreement. The *type* is still compared, because
/// filtering a `string` column with an `int` is a mistake either way.
fn resolve_filter(
    model: &AppModel,
    entity: &str,
    joined: Option<&str>,
    field: &str,
) -> Result<String> {
    let relaxed = field
        .split_once(':')
        .map_or_else(|| field.to_string(), |(name, _)| name.to_string());
    match resolve(model, entity, joined, field) {
        Ok(label) => Ok(label),
        // The bare name resolves against the entity's own declaration, so the
        // column still has to exist -- this relaxes the suffix, not the check.
        Err(error) => resolve(model, entity, joined, &relaxed).map_err(|_| error),
    }
}

fn resolve(model: &AppModel, entity: &str, joined: Option<&str>, field: &str) -> Result<String> {
    match operation_field_label(model, entity, field) {
        Ok(label) => Ok(label),
        // Qualified by the join's alias, because that is how the model names a
        // column that is not on this table: an unqualified `email` is checked
        // against the target and rejected, while `user.email` is the join's.
        Err(error) => match joined {
            Some(joined) => operation_field_label(model, joined, field)
                .map(|label| format!("{joined}.{label}"))
                .map_err(|_| error),
            None => Err(error),
        },
    }
}

/// The payload components of an event, where a typed token means something a
/// filter or an input cannot.
///
/// **An event is the one operation whose payload is not a subset of the row.**
/// It can carry a component the target does not have -- its own minted
/// identity, the moment it happened -- and JDL v1 spells that `name: type`,
/// against a bare `name` for a projection. Everywhere else a typed token is a
/// redundant restatement of a projection, checked against the entity field and
/// then collapsed to its label; here it is the only way to say the thing.
///
/// This is what makes `g usecase --yields` reachable: an outbox stages by a
/// *minted* `id`, and an event whose `id` is projected from the row makes
/// `on conflict (id) do nothing` discard the second event about that resource.
/// Without a spelling for the difference the flag writes a policy the model
/// then refuses.
pub(crate) fn event_component_declarations(
    model: &AppModel,
    entity: &str,
    fields: &[String],
) -> Result<Vec<String>> {
    fields
        .iter()
        .map(|token| match token.split_once(':') {
            Some((name, ty)) => {
                let parsed = parse_field(token)?;
                if parsed.primary_key
                    || parsed.unique
                    || parsed.indexed
                    || parsed.min_length.is_some()
                    || parsed.max_length.is_some()
                {
                    return Err(Failure::Told(format!(
                        "event component `{token}` carries a table constraint.\n       fix: an event is not stored -- use `{name}:{ty}` without `@pk`, `@unique`, `@index` or a range"
                    )));
                }
                Ok(format!("{}: {}", parsed.label, parsed.type_name))
            }
            None if entity.is_empty() => Err(Failure::Told(format!(
                "event component `{token}` has no type, and this event names no entity to read one from.\n       fix: write `{token}:<type>`, or pass `--on <Entity>` to borrow the row's"
            ))),
            None => operation_field_label(model, entity, token),
        })
        .collect()
}

pub(crate) fn operation_field_label(model: &AppModel, entity: &str, token: &str) -> Result<String> {
    let declaration = model
        .entities
        .values()
        .find(|candidate| candidate.label == entity)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{entity}` does not name a canonical entity.\n       fix: choose an entity declared under `[entities]`"
            ))
        })?;
    if !token.contains(':') {
        let label = java_to_label(token);
        if declaration.fields.iter().any(|field| field.label == label) {
            return Ok(label);
        }
        return Err(Failure::Told(format!(
            "`{token}` is not a field on `{entity}`.\n       fix: name an existing entity field"
        )));
    }
    let parsed = parse_field(token)?;
    let field = declaration
        .fields
        .iter()
        .find(|field| field.label == parsed.label)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{}` is not a field on `{entity}`.\n       fix: name an existing entity field",
                parsed.java_name
            ))
        })?;
    if parsed.primary_key
        || parsed.unique
        || parsed.indexed
        || parsed.min_length.is_some()
        || parsed.max_length.is_some()
    {
        return Err(Failure::Told(format!(
            "operation field `{token}` redeclares an entity constraint.\n       fix: use `{}:{}` without range, `@pk`, `@unique`, or `@index`",
            parsed.java_name,
            field.ty.canonical_name()
        )));
    }
    let expected_type = field.ty.canonical_name();
    if parsed.type_name != expected_type
        || parsed.required != field.required
        || parsed.non_blank != field.non_blank
    {
        return Err(Failure::Told(format!(
            "operation field `{token}` disagrees with canonical entity field `{entity}.{}`.\n       fix: use `{}:{expected_type}{}` or the bare field name",
            field.label,
            field.names.java_member,
            if !field.required {
                "?"
            } else if field.non_blank {
                "!"
            } else {
                ""
            }
        )));
    }
    Ok(parsed.label)
}

pub(crate) struct ParsedField {
    pub(crate) label: String,
    pub(crate) java_name: String,
    pub(crate) type_name: String,
    pub(crate) required: bool,
    pub(crate) non_blank: bool,
    pub(crate) primary_key: bool,
    pub(crate) unique: bool,
    pub(crate) indexed: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
    pub(crate) positive: bool,
    pub(crate) nonnegative: bool,
    pub(crate) scoped: bool,
    pub(crate) version: bool,
    pub(crate) default: Option<String>,
    pub(crate) updated: bool,
    pub(crate) mapped_column: Option<String>,
}

impl ParsedField {
    pub(crate) fn require_v1_for_rich_semantics(&self) -> Result<()> {
        let marker = if self.positive {
            Some("@positive")
        } else if self.nonnegative {
            Some("@nonnegative")
        } else if self.scoped {
            Some("@scope")
        } else if self.version {
            Some("@version")
        } else if self.default.is_some() {
            Some("@default")
        } else if self.updated {
            Some("@updated")
        } else {
            None
        };
        if let Some(marker) = marker {
            return Err(Failure::Told(format!(
                "field marker `{marker}` requires `jdl 1`.\n       fix: upgrade `.jails/model.jdl` to JDL v1 or author the field there"
            )));
        }
        Ok(())
    }
}

pub(crate) fn kind_name(kind: ArtifactKind) -> String {
    use clap::ValueEnum as _;
    kind.to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
