//! Lossless JDL edits for the familiar `jails generate` surface.

mod component;
mod edit;
pub(crate) mod facet;
pub(crate) mod index;
mod unit;
pub(crate) use component::{component_kind, component_stem};
use edit::insert_entity_member;
pub(crate) use edit::{
    insert_field, is_v1_source, jdl_edit_failure, remove_capability, remove_dependency,
    remove_entity, remove_operation, remove_setting, remove_unit, rename_entity, set_entity_active,
};

use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_generate::{ParsedField, PreparedMutation, finish_generation, parse_field};
use crate::model_resource::java_to_label;
use crate::{Invocation, model_generate};
use jails_model::{EntityId, ModelPatch, OperationId, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

const MODEL_PATH: &str = crate::model_command::JDL_PATH;

/// Every generator kind, and there is deliberately no `_` arm.
///
/// The match became exhaustive when the last four kinds got an answer, and
/// keeping it that way is worth more than the arm it replaces: a kind added
/// without deciding what a canonical project does with it is now a compile
/// error rather than a silent fall-through to the compatibility refusal.
pub(crate) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    match args.kind {
        ArtifactKind::Field => crate::model_resource::add_generated_field(args, invocation),
        ArtifactKind::Record
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Scaffold => run_entity(args, invocation),
        ArtifactKind::Factory => facet::run(args, invocation, facet::Kind::Factory),
        ArtifactKind::Dto => facet::run(args, invocation, facet::Kind::Dto),
        ArtifactKind::Repo => facet::run(args, invocation, facet::Kind::Repository),
        ArtifactKind::Usecase
        | ArtifactKind::Query
        | ArtifactKind::Transition
        | ArtifactKind::Event => run_operation(args, invocation),
        ArtifactKind::Class
        | ArtifactKind::Interface
        | ArtifactKind::Service
        | ArtifactKind::Handler
        | ArtifactKind::Command
        | ArtifactKind::Cli
        | ArtifactKind::Cases
        | ArtifactKind::Client
        | ArtifactKind::Fetcher
        | ArtifactKind::Job
        | ArtifactKind::HttpWorkflow
        | ArtifactKind::HttpSink
        | ArtifactKind::Idempotency
        | ArtifactKind::Auth
        | ArtifactKind::Webhook
        | ArtifactKind::DurableJob
        | ArtifactKind::Socket
        | ArtifactKind::Presence
        | ArtifactKind::Test
        | ArtifactKind::IntegrationTest
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Controller => unit::run(args, invocation),
        ArtifactKind::Migration
        | ArtifactKind::Association
        | ArtifactKind::Search
        | ArtifactKind::Seed => Err(Failure::Told(unsupported_kind(args.kind))),
    }
}

/// What is actually missing for the four kinds a canonical project refuses.
///
/// The generic refusal told the reader to edit `.jails/model.jdl` and run
/// `jails sync`. That is false advice for every one of these: the model
/// carries the *vocabulary* -- `ProjectionKind::Search { fields }`,
/// `ProjectionKind::Seed`, `AppModel.relations` -- and no emitter reads it, so
/// hand-editing the JDL produces a valid model and no artifact. `plan.md`
/// P13.8 has the measurement. A `fix:` line naming a repair that does not
/// repair is worse than no fix line, which is why
/// `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
/// exists.
fn unsupported_kind(kind: ArtifactKind) -> String {
    let (what, detail): (&str, String) = match kind {
        ArtifactKind::Migration => (
            "a hand-written migration",
            "a migration nobody derived is an irreproducible operation, and the canonical plan has no seam for one yet -- only the schema diff appends migrations.\n       fix: write the file yourself under `src/main/resources/db/migration`; canonical capture reads what is there".to_string(),
        ),
        ArtifactKind::Association => (
            "an association",
            "`AppModel` carries relations and no emitter reads them, so nothing renders the foreign key.\n       fix: write the constraint as a migration by hand for now".to_string(),
        ),
        ArtifactKind::Search => (
            "full-text search",
            "the compiler emits the search port and not the `tsvector` column, the GIN index or the JDBC adapter, so declaring it would leave three quarters missing.\n       fix: write the column, index and adapter by hand for now".to_string(),
        ),
        ArtifactKind::Seed => (
            "seed data",
            "`ProjectionKind::Seed` links and no emitter reads it, so the model would accept the declaration and write nothing.\n       fix: write the seed file and its runner by hand for now".to_string(),
        ),
        other => (
            "this generator",
            format!(
                "`{other:?}` has no canonical backend.\n       fix: keep it outside the canonical model"
            ),
        ),
    };
    format!("canonical `generate` has no backend for {what}: {detail}")
}

fn run_operation(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    let profile = model_generate::operation_profile(args.kind).ok_or_else(|| {
        Failure::Told(
            "this JDL generator is not an operation declaration\n       fix: use `usecase`, `query`, `transition`, or `event`"
                .to_string(),
        )
    })?;
    model_generate::reject_unsupported_operation_options(&args, profile)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let v1 = is_v1_source(&current_source);
    let current_model = parse(&current_source)?;
    let on = args
        .strategy_on
        .as_deref()
        .expect("operation validation requires --on");
    let requested_entity = java_to_label(on);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == requested_entity || entity.names.java_type == on)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{on}` does not name a canonical entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
            ))
        })?;
    let entity_label = entity.label.clone();
    let entity_java_name = entity.names.java_type.clone();
    let fields =
        model_generate::operation_field_labels(&current_model, &entity_label, &args.fields)?;
    let operation_label = java_to_label(&args.name);
    let operation_id = OperationId::parse(format!("op_{operation_label}"))
        .map_err(|error| Failure::Told(format!("could not assign operation identity: {error}")))?;
    let declaration = operation_declaration(&args, &current_model, &entity_label, &fields, v1)?;
    if let Some(existing) = current_model.operations.get(&operation_id) {
        let without = remove_operation(&current_source, &args.name, operation_id.as_str())?;
        let requested_source = insert_entity_member(&without, &entity_java_name, &declaration)?;
        let requested_model = parse(&requested_source)?;
        let requested = requested_model
            .operations
            .get(&operation_id)
            .ok_or_else(|| {
                Failure::Told(format!("requested operation `{operation_id}` did not link"))
            })?;
        if existing != requested {
            return Err(Failure::Told(format!(
                "canonical operation `{}` is already declared with a different shape.\n       fix: remove it explicitly before changing its semantic contract",
                args.name
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
        });
    }
    let next_source = insert_entity_member(&current_source, &entity_java_name, &declaration)?;
    let next_model = parse(&next_source)?;
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

fn operation_declaration(
    args: &GenerateArgs,
    model: &jails_model::AppModel,
    entity_label: &str,
    fields: &[String],
    v1: bool,
) -> Result<String> {
    let kind = match args.kind {
        ArtifactKind::Usecase => "command",
        ArtifactKind::Query => "query",
        ArtifactKind::Transition => "transition",
        ArtifactKind::Event => "event",
        _ => unreachable!("operation generation accepts only operation kinds"),
    };
    let mut output = format!(
        "  {kind} {}({}) @id(op_{}) {{\n",
        args.name,
        fields.join(", "),
        java_to_label(&args.name)
    );
    if args.kind == ArtifactKind::Query {
        if let Some(order_by) = &args.order_by {
            let order_by = order_by
                .split(',')
                .map(str::trim)
                .map(|item| {
                    if item.is_empty() || item.contains(char::is_whitespace) {
                        return Err(Failure::Told(format!(
                            "canonical query ordering does not yet represent directions in `{item}`.\n       fix: use a comma-separated field list without `asc`/`desc`"
                        )));
                    }
                    model_generate::operation_field_label(model, entity_label, item)
                })
                .collect::<Result<Vec<_>>>()?;
            if v1 {
                output.push_str(&format!("    order by [{}]\n", order_by.join(", ")));
            } else {
                output.push_str(&format!("    orderBy: {}\n", order_by.join(", ")));
            }
        }
        if let Some(limit) = args.limit {
            if v1 {
                output.push_str(&format!("    limit {limit}\n"));
            } else {
                output.push_str(&format!("    limit: {limit}\n"));
            }
        }
    }
    if args.kind == ArtifactKind::Transition {
        if v1 {
            output.push_str(&format!("    update [{}]\n", fields.join(", ")));
        } else {
            output.push_str(&format!("    sets: {}\n", fields.join(", ")));
        }
        if let Some(yields) = &args.strategy_yields {
            if v1 {
                output.push_str(&format!("    emit {}\n", java_to_label(yields)));
            } else {
                output.push_str(&format!("    yields: {}\n", java_to_label(yields)));
            }
        }
    }
    if let Some(path) = &args.path {
        let method = match args.kind {
            ArtifactKind::Usecase => "POST".to_string(),
            ArtifactKind::Query if fields.is_empty() => "GET".to_string(),
            ArtifactKind::Query => "POST".to_string(),
            ArtifactKind::Transition => args.method.map_or_else(
                || "PUT".to_string(),
                |method| method.label().to_ascii_uppercase(),
            ),
            _ => unreachable!("event paths are rejected during validation"),
        };
        if v1 {
            let path = serde_json::to_string(path)
                .map_err(|error| Failure::Told(format!("could not quote route path: {error}")))?;
            output.push_str(&format!("    route {method} {path}\n"));
        } else {
            output.push_str(&format!("    route: {method} {path}\n"));
        }
    }
    output.push_str("  }");
    Ok(output)
}

fn run_entity(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    model_generate::validate_entity_args(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let v1 = is_v1_source(&current_source);
    let current_model = parse(&current_source)?;
    let entity_label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{entity_label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let mut fields = args.fields.clone();
    if args.timestamps {
        fields.extend(if v1 {
            [
                "createdAt:instant@default(now())".to_string(),
                "updatedAt:instant@default(now())@updated".to_string(),
            ]
        } else {
            [
                "createdAt:instant".to_string(),
                "updatedAt:instant".to_string(),
            ]
        });
    }
    let declaration = match args.kind {
        ArtifactKind::Enum => enum_declaration(&args.name, &entity_label, &fields, v1)?,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Scaffold => entity_declaration(
            &args.name,
            &entity_label,
            args.kind == ArtifactKind::Scaffold,
            &fields,
            v1,
        )?,
        _ => unreachable!("run only accepts entity kinds"),
    };
    if let Some(existing) = current_model.entity(&entity_id) {
        let requested = declaration_entity(&current_model, &declaration, &entity_id, v1)?;
        if !same_entity_contribution(existing, &requested) {
            return Err(Failure::Told(format!(
                "canonical entity `{}` is already declared with a different shape.\n       fix: evolve it with `jails g field`, `jails resource field`, or `jails rename resource`",
                args.name
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
        });
    }
    let next_source = append_declaration(current_source.clone(), &declaration)?;
    let next_model = parse(&next_source)?;
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

fn declaration_entity(
    model: &jails_model::AppModel,
    declaration: &str,
    entity_id: &EntityId,
    v1: bool,
) -> Result<jails_model::Entity> {
    let storage = match model.project.dialect.as_str() {
        "postgresql" => "postgres",
        "h2" => "h2",
        "sqlite" => "sqlite",
        _ => "none",
    };
    let source = if v1 {
        format!(
            "jdl 1\napp Comparison @id(project_comparison) {{\n  pkg {}\n  java {}\n  platform {}\n  build {}\n  storage {storage}\n}}\n\n{}",
            model.project.base_package,
            model.project.java_release,
            model.project.platform,
            model.project.build,
            declaration
        )
    } else {
        format!(
            "application Comparison @id(project_comparison)\npackage {}\njava {}\ndialect {}\n\n{}",
            model.project.base_package,
            model.project.java_release,
            model.project.dialect,
            declaration
        )
    };
    parse(&source)?
        .entity(entity_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("requested entity `{entity_id}` did not link")))
}

fn same_entity_contribution(
    existing: &jails_model::Entity,
    requested: &jails_model::Entity,
) -> bool {
    existing.id == requested.id
        && existing.label == requested.label
        && existing.names == requested.names
        && existing.active == requested.active
        && existing.facets == requested.facets
        && existing.enum_constants == requested.enum_constants
        && requested
            .fields
            .iter()
            .all(|field| existing.field(&field.id) == Some(field))
        && requested
            .indexes
            .iter()
            .all(|(id, index)| existing.indexes.get(id) == Some(index))
}

fn read_model() -> Result<String> {
    std::fs::read_to_string(MODEL_PATH).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{MODEL_PATH}`: {error}"
        ))
    })
}

fn parse(source: &str) -> Result<jails_model::AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

fn append_declaration(mut source: String, declaration: &str) -> Result<String> {
    if is_v1_source(&source) {
        return jails_model::append_jdl_declaration(&source, declaration).map_err(jdl_edit_failure);
    }
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push('\n');
    source.push_str(declaration);
    Ok(source)
}

pub(crate) fn entity_declaration(
    java_name: &str,
    entity_label: &str,
    scaffold: bool,
    fields: &[String],
    v1: bool,
) -> Result<String> {
    let mut labels = BTreeSet::new();
    let mut parsed = Vec::new();
    for token in fields {
        let field = parse_field(token)?;
        if !labels.insert(field.label.clone()) {
            return Err(Failure::Told(format!(
                "field `{}` is declared more than once\n       fix: keep one declaration for each field name",
                field.java_name
            )));
        }
        parsed.push(field);
    }
    let mut output = format!("entity {java_name} @id(ent_{entity_label}) {{\n");
    if scaffold {
        if v1 {
            output.push_str("  use scaffold\n\n");
        } else {
            output = output.replacen(" {", " @scaffold {", 1);
        }
    }
    for field in &parsed {
        let line = if v1 {
            render_v1_field_line(entity_label, field)
        } else {
            render_field_line(entity_label, field)?
        };
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn enum_declaration(
    java_name: &str,
    label: &str,
    values: &[String],
    v1: bool,
) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_protocol::declaration::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = format!("enum {java_name} @id(ent_{label}) {{\n");
    for value in values {
        output.push_str("  ");
        if v1 {
            if let Some((constant, wire)) = value.split_once('=') {
                output.push_str(constant);
                output.push_str(" = ");
                output.push_str(&serde_json::to_string(wire).map_err(|error| {
                    Failure::Told(format!("could not quote enum wire value: {error}"))
                })?);
            } else {
                output.push_str(&value);
            }
        } else {
            output.push_str(&value);
        }
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn render_field_line(entity_label: &str, field: &ParsedField) -> Result<String> {
    field.require_v1_for_rich_semantics()?;
    let suffix = if !field.required {
        "?"
    } else if field.non_blank {
        "!"
    } else {
        ""
    };
    let range = if field.min_length.is_some() || field.max_length.is_some() {
        format!(
            "({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )
    } else {
        String::new()
    };
    let mut output = format!(
        "  {}: {}{}{} @id(fld_{}_{})",
        field.java_name, field.type_name, suffix, range, entity_label, field.label
    );
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if let Some(column) = &field.mapped_column {
        output.push_str(&format!(" @column({column})"));
    }
    Ok(output)
}

pub(crate) fn render_v1_field_line(entity_label: &str, field: &ParsedField) -> String {
    let optional = if field.required { "" } else { "?" };
    let mut output = format!(
        "  {}: {}{} @id(fld_{}_{})",
        field.java_name, field.type_name, optional, entity_label, field.label
    );
    if let Some(default) = &field.default {
        output.push_str(&format!(" @default({default})"));
    }
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.version {
        output.push_str(" @version");
    }
    if field.min_length.is_some() || field.max_length.is_some() {
        output.push_str(&format!(
            " @length({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ));
    }
    if field.nonnegative {
        output.push_str(" @nonnegative");
    }
    if field.non_blank {
        output.push_str(" @notBlank");
    }
    if field.positive {
        output.push_str(" @positive");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if field.scoped {
        output.push_str(" @scope");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.updated {
        output.push_str(" @updated");
    }
    if let Some(column) = &field.mapped_column {
        let column = serde_json::to_string(column).expect("string serialization cannot fail");
        output.push_str(&format!(" @map({column})"));
    }
    output
}
