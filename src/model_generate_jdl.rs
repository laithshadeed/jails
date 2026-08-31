//! Lossless JDL edits for the familiar `jails generate` surface.

mod component;
mod edit;
pub(crate) mod facet;
pub(crate) mod index;
mod relation;
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
use std::path::{Path, PathBuf};

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
        ArtifactKind::Seed => facet::run(args, invocation, facet::Kind::Seed),
        ArtifactKind::Search => facet::run(args, invocation, facet::Kind::Search),
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
        ArtifactKind::Association => relation::run(args, invocation),
        ArtifactKind::Migration => crate::model_migration::run(args, invocation),
    }
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
    let current_source = read_model(&invocation)?;
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
    // An event's payload can carry a component the row does not: see
    // `event_component_declarations`. Every other operation's field list is a
    // projection of the target, so it goes through the checked resolver.
    let fields = if args.kind == ArtifactKind::Event {
        model_generate::event_component_declarations(&current_model, &entity_label, &args.fields)?
    } else {
        model_generate::operation_field_labels_via(
            &current_model,
            &entity_label,
            args.via
                .as_deref()
                .map(java_type_name)
                .as_deref()
                .map(java_to_label)
                .as_deref(),
            args.kind == ArtifactKind::Query,
            &args.fields,
        )?
    };
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
            authored_migration: None,
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
        authored_migration: None,
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
                // **`asc`/`desc` pass through.** `operation_order_list`
                // has parsed a direction since the grammar existed --
                // `order by [ timeStamp desc ]` -- and this refused to emit
                // one, so a query whose whole point is "newest first" could
                // not reach a canonical project. The field still goes through
                // the checked resolver; only the direction rides beside it.
                .map(|item| {
                    let (field, direction) = match item.split_once(char::is_whitespace) {
                        Some((field, rest)) => (field, rest.trim()),
                        None => (item, ""),
                    };
                    if field.is_empty() {
                        return Err(Failure::Told(
                            "canonical query ordering needs a field name.\n       fix: give `--order-by` a comma-separated field list"
                                .to_string(),
                        ));
                    }
                    let direction = match direction {
                        "" | "asc" => "",
                        "desc" => " desc",
                        other => {
                            return Err(Failure::Told(format!(
                                "`{other}` is not an ordering direction.\n       fix: use `asc` or `desc`"
                            )));
                        }
                    };
                    let label =
                        model_generate::operation_field_label(model, entity_label, field)?;
                    Ok(format!("{label}{direction}"))
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
    if args.kind == ArtifactKind::Usecase
        && let Some(yields) = &args.strategy_yields
    {
        // `--yields` on a use case is the legacy spelling of *staged*
        // delivery: it is what `g usecase --yields E` has always built an
        // outbox for. Writing `emit` alone would honour the flag with direct
        // publication, which is the weaker guarantee and the exact
        // substitution `deliver` exists to make impossible.
        let event = java_to_label(yields);
        if v1 {
            output.push_str(&format!("    emit {event}\n    deliver outbox\n"));
        } else {
            output.push_str(&format!("    emits: {event}\n    delivery: outbox\n"));
        }
    }
    // `--via` is a `join`: `g query --via User` reads `users` alongside
    // `messages`, on the `userId` the child already declares. The model has
    // carried `Query.semantics.joins` and the JDL has parsed
    // `join User as user on userId -> user.id` all along; only this frontend
    // refused to translate the flag.
    //
    // The column is derived from the two entities rather than recorded, which
    // is the legacy `join` module's rule: `<parent>Id` on the child, and the
    // parent's own primary key on the other side. A reference the model does
    // not declare is named rather than guessed at.
    if args.kind == ArtifactKind::Query
        && let Some(via) = &args.via
    {
        let parent = java_type_name(via);
        let parent_label = java_to_label(&parent);
        let parent_entity = model
            .entities
            .values()
            .find(|entity| entity.label == parent_label)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent}` does not name a canonical entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
                ))
            })?;
        let key = parent_entity
            .fields
            .iter()
            .find(|field| field.primary_key)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent}` has no primary key, so nothing can join to it.\n       fix: declare one component `@pk`"
                ))
            })?;
        let child = model
            .entities
            .values()
            .find(|entity| entity.label == entity_label)
            .and_then(|entity| {
                entity
                    .fields
                    .iter()
                    .find(|field| field.label == format!("{parent_label}_id"))
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{}` declares no `{parent_label}_id` component, so it does not reference `{parent}`.\n       fix: add one, or drop `--via {parent}`",
                    args.name
                ))
            })?;
        let alias = &parent_label;
        if v1 {
            output.push_str(&format!(
                "    join {parent} as {alias} on {} -> {alias}.{}\n",
                child.label, key.label
            ));
        } else {
            output.push_str(&format!(
                "    via: {parent}\n    join_on: {} -> {}\n",
                child.label, key.label
            ));
        }
    }
    // `--on-conflict` is `conflict on [field]`: one
    // `insert ... on conflict (col) do nothing returning`, then a read of the
    // row that was already there. The model has carried `conflict_key` and the
    // JDL has parsed `conflict on [...]` all along; only this frontend refused
    // to translate the flag, so `g usecase --on-conflict` could not reach a
    // canonical project at all.
    if args.kind == ArtifactKind::Usecase
        && let Some(component) = &args.on_conflict
    {
        let label = model_generate::operation_field_label(model, entity_label, component)?;
        if v1 {
            output.push_str(&format!("    conflict on [{label}]\n"));
        } else {
            output.push_str(&format!("    conflict_on: {label}\n"));
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

/// A CLI name, as the Java type it names.
///
/// `jails g enum currency GBP EUR` writes `Currency.java` on the legacy path
/// and every generator that later says `currency:Currency` resolves against
/// it -- which is the whole of
/// `generators_compose_through_user_owned_field_types`. The canonical model
/// requires a real Java type name and refused the lower-camel spelling
/// outright, so the same command produced a project on one engine and a
/// diagnostic on the other.
///
/// Capitalising here rather than loosening the model: `java_name` is a
/// projection the model is right to hold to, and this is the CLI sugar
/// resolving what the reader typed, which is where the legacy path does it
/// too.
pub(super) fn java_type_name(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn run_entity(mut args: GenerateArgs, invocation: Invocation) -> Result<()> {
    args.name = java_type_name(&args.name);
    model_generate::validate_entity_args(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
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
            authored_migration: None,
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
    let indexes = std::mem::take(&mut args.indexes);
    let name = args.name.clone();
    finish_generation(PreparedMutation {
        name: args.name,
        invocation: invocation.clone(),
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddEntity(entity),
        patch_bytes,
        authored_migration: None,
    })?;
    // **`--index` is a second patch, not a flag on the first.** An index is a
    // stable entity child with its own identity and its own forward
    // migration, which is `resource index add`'s whole contract -- so the
    // frontend that owns it is the one that applies it, rather than the entity
    // renderer growing a copy. Refusing instead was what stopped a proof
    // application's `g scaffold --index "user_id, time_stamp desc"` from
    // reaching the canonical path at all.
    //
    // After the entity, necessarily: the columns are resolved against model
    // field identity, and the fields do not exist until the patch above lands.
    for columns in indexes {
        crate::model_index::add(name.clone(), columns, None, invocation.clone())?;
    }
    Ok(())
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
        // **A subset, like the fields and indexes below.** This asked for
        // equality, so re-declaring an entity that had since gained a facet
        // refused -- "already declared with a different shape" -- even though
        // everything the request asks for is present. The minicom manifest
        // declares `User` as a scaffold and again as a seed, so replaying it
        // a second time hit exactly that, and the function's own name is
        // `contribution`: what this request contributes must be there, not
        // everything that is there must have come from this request.
        && requested.facets.is_subset(&existing.facets)
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

pub(crate) fn read_model(invocation: &Invocation) -> Result<String> {
    crate::model_command::read_source_at(&invocation.root()?, Path::new(MODEL_PATH))
}

pub(crate) fn parse(source: &str) -> Result<jails_model::AppModel> {
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
