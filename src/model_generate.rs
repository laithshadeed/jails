//! Compatibility lowering from familiar generation syntax into `ModelPatch`.

#[path = "model_field_parse.rs"]
mod field_parse;
pub(crate) use field_parse::{normalize_type, parse_field};

use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_resource::java_to_label;
use crate::{Invocation, Output};
use jails_contracts::{CanonicalModelPatch, ModelFileUpdate, ProjectPath};
use jails_model::{AppModel, EntityId, Facet, ModelPatch, OperationId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MODEL_PATH: &str = ".jails/model.toml";

pub(crate) fn owns(_args: &GenerateArgs) -> bool {
    crate::model_command::owns()
}

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
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{MODEL_PATH}`: {error}"
        ))
    })?;
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
    })
}

fn run_operation(
    args: GenerateArgs,
    profile: OperationProfile,
    invocation: Invocation,
) -> Result<()> {
    reject_unsupported_operation_options(&args, profile)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{MODEL_PATH}`: {error}"
        ))
    })?;
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
    } = prepared;
    let canonical_model_path = model_path.to_string_lossy().replace('\\', "/");
    let root = std::env::current_dir()
        .map_err(|error| Failure::Told(format!("could not read current directory: {error}")))?;
    let mut next_model = current_model.clone();
    next_model
        .apply(patch.clone())
        .map_err(|error| Failure::Told(format!("could not prepare model capture: {error}")))?;
    let mut capture_paths = reader_paths.to_vec();
    capture_paths.extend(jails_compiler::external_project_paths(&next_model));
    capture_paths.sort();
    capture_paths.dedup();
    let snapshot = jails_workspace::capture_with_reader_paths(
        &root,
        &model_path,
        current_source.as_bytes(),
        current_model,
        &capture_paths,
    )
    .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let draft = jails_compiler::Compiler::compile(&snapshot, Some(patch))
        .map_err(|error| Failure::Told(format!("could not compile model patch: {error}")))?;
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
}

fn entity_profile(kind: ArtifactKind) -> Option<&'static EntityProfile> {
    static RECORD: EntityProfile = EntityProfile {
        facets: RECORD_FACETS,
        timestamps: false,
    };
    static ENUM: EntityProfile = EntityProfile {
        facets: ENUM_FACETS,
        timestamps: false,
    };
    static SCAFFOLD: EntityProfile = EntityProfile {
        facets: SCAFFOLD_FACETS,
        timestamps: true,
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
        || !args.indexes.is_empty()
        || args.strategy_on.is_some()
        || args.strategy_yields.is_some()
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || args.path.is_some()
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
        || args.via.is_some()
        || args.on_conflict.is_some()
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || !args.bind.is_empty()
        || args.consumes.is_some()
        || (args.order_by.is_some() && profile != OperationProfile::Query)
        || (args.limit.is_some() && profile != OperationProfile::Query)
        || (args.strategy_yields.is_some() && profile != OperationProfile::Transition)
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

fn operation_declaration(
    args: &GenerateArgs,
    profile: OperationProfile,
    model: &AppModel,
    label: &str,
) -> Result<String> {
    let on = args
        .strategy_on
        .as_deref()
        .expect("operation option validation requires --on");
    let on = java_to_label(on);
    let fields = operation_field_labels(model, &on, &args.fields)?;
    let fields = quoted_array(&fields)?;
    let mut output = format!(
        "[operations.{label}]\nkind = {}\nid = {}\njava_name = {}\non = {}\n",
        quoted(operation_kind(profile))?,
        quoted(&format!("op_{label}"))?,
        quoted(&args.name)?,
        quoted(&on)?,
    );
    match profile {
        OperationProfile::Command => {
            output.push_str(&format!("fields = {fields}\n"));
        }
        OperationProfile::Query => {
            output.push_str(&format!("filters = {fields}\n"));
            if let Some(order_by) = &args.order_by {
                let order_by = order_by
                    .split(',')
                    .map(str::trim)
                    .map(|item| {
                        if item.is_empty() || item.contains(char::is_whitespace) {
                            return Err(Failure::Told(format!(
                                "canonical query ordering does not yet represent directions in `{item}`.\n       fix: use a comma-separated field list without `asc`/`desc`, or declare the query directly in `{MODEL_PATH}`"
                            )));
                        }
                        operation_field_label(model, &on, item)
                    })
                    .collect::<Result<Vec<_>>>()?;
                output.push_str(&format!("order_by = {}\n", quoted_array(&order_by)?));
            }
            if let Some(limit) = args.limit {
                output.push_str(&format!("limit = {limit}\n"));
            }
        }
        OperationProfile::Transition => {
            output.push_str(&format!("fields = {fields}\nsets = {fields}\n"));
            if let Some(yields) = &args.strategy_yields {
                output.push_str(&format!("yields = {}\n", quoted(&java_to_label(yields))?));
            }
        }
        OperationProfile::Event => {
            output.push_str(&format!("fields = {fields}\n"));
        }
    }
    if let Some(path) = &args.path {
        let method = match profile {
            OperationProfile::Command => "POST".to_string(),
            OperationProfile::Query if args.fields.is_empty() => "GET".to_string(),
            OperationProfile::Query => "POST".to_string(),
            OperationProfile::Transition => args.method.map_or_else(
                || "PUT".to_string(),
                |method| method.label().to_ascii_uppercase(),
            ),
            OperationProfile::Event => unreachable!("event paths are refused"),
        };
        output.push_str(&format!(
            "route = {}\n",
            quoted(&format!("{method} {path}"))?
        ));
    }
    Ok(output)
}

fn operation_kind(profile: OperationProfile) -> &'static str {
    match profile {
        OperationProfile::Command => "command",
        OperationProfile::Query => "query",
        OperationProfile::Transition => "transition",
        OperationProfile::Event => "event",
    }
}

pub(crate) fn operation_field_labels(
    model: &AppModel,
    entity: &str,
    fields: &[String],
) -> Result<Vec<String>> {
    fields
        .iter()
        .map(|field| operation_field_label(model, entity, field))
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

fn quoted_array(values: &[String]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(|value| quoted(value))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
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

pub(crate) fn entity_declaration(
    label: &str,
    java_name: &str,
    facets: &[Facet],
    fields: &[String],
) -> Result<String> {
    let mut parsed = Vec::new();
    let mut labels = BTreeSet::new();
    for token in fields {
        let field = parse_field(token)?;
        if !labels.insert(field.label.clone()) {
            return Err(Failure::Told(format!(
                "field `{}` is declared more than once",
                field.java_name
            )));
        }
        parsed.push(field);
    }
    let facets = facets
        .iter()
        .map(|facet| quoted(facet_name(*facet)))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let mut output = format!(
        "[entities.{label}]\nid = {}\njava_name = {}\nfacets = [{facets}]\n",
        quoted(&format!("ent_{label}"))?,
        quoted(java_name)?,
    );
    for field in parsed {
        output.push('\n');
        output.push_str(&field_declaration(label, &field)?);
    }
    Ok(output)
}

pub(crate) fn enum_declaration(label: &str, java_name: &str, values: &[String]) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_protocol::declaration::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "[entities.{label}]\nid = {}\njava_name = {}\nfacets = [\"enum\"]\nvalues = {}\n",
        quoted(&format!("ent_{label}"))?,
        quoted(java_name)?,
        quoted_array(&values)?,
    ))
}

pub(crate) fn field_declaration(entity: &str, field: &ParsedField) -> Result<String> {
    field.require_v1_for_rich_semantics()?;
    let mut output = format!(
        "[entities.{entity}.fields.{}]\nid = {}\njava_name = {}\ntype = {}\nrequired = {}\nnon_blank = {}\nprimary_key = {}\nunique = {}\nindexed = {}\n",
        field.label,
        quoted(&format!("fld_{entity}_{}", field.label))?,
        quoted(&field.java_name)?,
        quoted(&field.type_name)?,
        field.required,
        field.non_blank,
        field.primary_key,
        field.unique,
        field.indexed,
    );
    if let Some(min) = field.min_length {
        output.push_str(&format!("min_length = {min}\n"));
    }
    if let Some(max) = field.max_length {
        output.push_str(&format!("max_length = {max}\n"));
    }
    if let Some(column) = &field.mapped_column {
        output.push_str(&format!("column = {}\n", quoted(column)?));
    }
    Ok(output)
}

fn facet_name(facet: Facet) -> &'static str {
    match facet {
        Facet::Enum => "enum",
        Facet::Record => "record",
        Facet::Factory => "factory",
        Facet::Dto => "dto",
        Facet::Repository => "repository",
        Facet::Service => "service",
        Facet::Http => "http",
        Facet::Events => "events",
        Facet::Search => "search",
        Facet::Seed => "seed",
    }
}

fn quoted(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}

fn kind_name(kind: ArtifactKind) -> String {
    use clap::ValueEnum as _;
    kind.to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
