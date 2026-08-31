//! Compatibility lowering from familiar generation syntax into `ModelPatch`.

#[path = "model_field_parse.rs"]
mod field_parse;
pub(crate) use field_parse::{normalize_type, parse_field};

mod render;
use render::operation_declaration;
pub(crate) use render::{entity_declaration, enum_declaration, field_declaration};

use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_resource::java_to_label;
use crate::{Invocation, Output};
use jails_contracts::{CanonicalModelPatch, ModelFileUpdate, ProjectPath};
use jails_model::{AppModel, EntityId, Facet, ModelPatch, OperationId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

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
    let mut draft = jails_compiler::Compiler::compile(&snapshot, Some(patch))
        .map_err(|error| Failure::Told(format!("could not compile model patch: {error}")))?;
    // After the compile, because it is not derived from the model: see
    // `PreparedMutation::authored_migration`. It is still the plan's, not a
    // side effect -- the materializer allocates its version from the observed
    // history and refuses if the path it lands on already exists.
    draft.migrations.extend(authored_migration);
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
        jails_workspace::Restore::Refuse,
    )
    .map_err(|error| Failure::Told(format!("could not materialize exact plan: {error}")))?;

    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply exact plan: {error}")))?;
    if invocation.output == Output::Human {
        println!(
            "applied model patch for {}: {} ({} files written)",
            name,
            execution.plan_digest.as_str(),
            execution.files_written
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution)
                .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?
        );
    }
    Ok(())
}

pub(crate) fn report_plan(
    bundle: &jails_contracts::PlanBundle,
    invocation: &Invocation,
) -> Result<()> {
    if invocation.output == Output::Human {
        println!(
            "plan {}: {} operations, {} managed files",
            bundle.plan.digest.as_str(),
            bundle.plan.operations.len(),
            bundle.plan.summary.managed_files
        );
        if invocation.ast {
            println!(
                "model patch: {}",
                String::from_utf8_lossy(&bundle.plan.input.bytes)
            );
        }
        if invocation.diff {
            for operation in &bundle.plan.operations {
                println!("  {operation:?}");
            }
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(bundle)
                .map_err(|error| Failure::Told(format!("could not encode exact plan: {error}")))?
        );
    }
    Ok(())
}

pub(crate) fn write_bundle(path: &Path, bundle: &jails_contracts::PlanBundle) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(bundle)
        .map_err(|error| Failure::Told(format!("could not encode exact plan: {error}")))?;
    jails_support::apply::put_outside_project_private_atomic(path, encoded)
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
        route: false,
    };
    static ENUM: EntityProfile = EntityProfile {
        facets: ENUM_FACETS,
        timestamps: false,
        route: false,
    };
    static SCAFFOLD: EntityProfile = EntityProfile {
        facets: SCAFFOLD_FACETS,
        timestamps: true,
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

fn reject_unsupported_options(args: &GenerateArgs, profile: &EntityProfile) -> Result<()> {
    let unsupported = (args.timestamps && !profile.timestamps)
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || args.strategy_on.is_some()
        || args.strategy_yields.is_some()
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || (args.path.is_some() && !profile.route)
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || !args.bind.is_empty()
        || args.method.is_some()
        || args.consumes.is_some();
    if unsupported {
        return Err(Failure::Told(format!(
            "the canonical `{}` semantic profile does not represent one or more supplied flags.\n       fix: remove the unsupported flags and put semantic projections in `.jails/model.toml`",
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
    reject_unsupported_options(args, profile)
}

pub(crate) fn reject_unsupported_operation_options(
    args: &GenerateArgs,
    profile: OperationProfile,
) -> Result<()> {
    let unsupported = args.timestamps
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || (args.on_conflict.is_some() && profile != OperationProfile::Command)
        || (args.via.is_some()
            && !matches!(profile, OperationProfile::Query | OperationProfile::Command))
        || (args.select.is_some() && profile != OperationProfile::Transition)
        || (!args.set.is_empty()
            && !matches!(
                profile,
                OperationProfile::Transition | OperationProfile::Command
            ))
        || (args.if_match.is_some() && profile != OperationProfile::Transition)
        || (!args.bind.is_empty() && profile == OperationProfile::Event)
        || (args.consumes.is_some() && profile == OperationProfile::Event)
        || (args.order_by.is_some() && profile != OperationProfile::Query)
        || (args.limit.is_some() && profile != OperationProfile::Query)
        || (args.strategy_yields.is_some()
            && !matches!(
                profile,
                OperationProfile::Transition | OperationProfile::Command
            ))
        || (args.method.is_some() && profile != OperationProfile::Transition)
        || (args.path.is_some() && profile == OperationProfile::Event);
    if unsupported {
        return Err(Failure::Told(format!(
            "the canonical `{}` operation frontend does not represent one or more supplied flags.\n       fix: remove the unsupported flags or declare the typed operation directly in `{MODEL_PATH}`",
            kind_name(args.kind)
        )));
    }
    if args.strategy_on.is_none() {
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

fn kind_name(kind: ArtifactKind) -> String {
    use clap::ValueEnum as _;
    kind.to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
