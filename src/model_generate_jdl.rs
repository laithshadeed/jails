//! Lossless JDL edits for the familiar `jails generate` surface.

pub(crate) mod facet;
pub(crate) mod index;
mod unit;

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
        | ArtifactKind::Test
        | ArtifactKind::IntegrationTest
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Controller => unit::run(args, invocation),
        _ => crate::model_command::require_toml_mutation("generate"),
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
    let current_source = read_model()?;
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
    let declaration = operation_declaration(&args, &current_model, &entity_label, &fields)?;
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
            output.push_str(&format!("    orderBy: {}\n", order_by.join(", ")));
        }
        if let Some(limit) = args.limit {
            output.push_str(&format!("    limit: {limit}\n"));
        }
    }
    if args.kind == ArtifactKind::Transition {
        output.push_str(&format!("    sets: {}\n", fields.join(", ")));
        if let Some(yields) = &args.strategy_yields {
            output.push_str(&format!("    yields: {}\n", java_to_label(yields)));
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
        output.push_str(&format!("    route: {method} {path}\n"));
    }
    output.push_str("  }");
    Ok(output)
}

fn run_entity(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    model_generate::validate_entity_args(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let current_model = parse(&current_source)?;
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
    let declaration = match args.kind {
        ArtifactKind::Enum => enum_declaration(&args.name, &entity_label, &fields)?,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Scaffold => entity_declaration(
            &args.name,
            &entity_label,
            args.kind == ArtifactKind::Scaffold,
            &fields,
        )?,
        _ => unreachable!("run only accepts entity kinds"),
    };
    if let Some(existing) = current_model.entity(&entity_id) {
        let requested = declaration_entity(&current_model, &declaration, &entity_id)?;
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
    let next_source = append_declaration(current_source.clone(), &declaration);
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
) -> Result<jails_model::Entity> {
    let source = format!(
        "application Comparison @id(project_comparison)\npackage {}\njava {}\ndialect {}\n\n{}",
        model.project.base_package, model.project.java_release, model.project.dialect, declaration
    );
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
            .all(|(id, field)| existing.fields.get(id) == Some(field))
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

fn append_declaration(mut source: String, declaration: &str) -> String {
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push('\n');
    source.push_str(declaration);
    source
}

pub(crate) fn entity_declaration(
    java_name: &str,
    entity_label: &str,
    scaffold: bool,
    fields: &[String],
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
    let scaffold = if scaffold { " @scaffold" } else { "" };
    let mut output = format!("entity {java_name} @id(ent_{entity_label}){scaffold} {{\n");
    for field in &parsed {
        output.push_str(&render_field_line(entity_label, field));
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn enum_declaration(java_name: &str, label: &str, values: &[String]) -> Result<String> {
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
        output.push_str(&value);
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn render_field_line(entity_label: &str, field: &ParsedField) -> String {
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
    output
}

pub(crate) fn insert_field(
    source: &str,
    entity_java_name: &str,
    field_line: &str,
) -> Result<String> {
    insert_entity_member(source, entity_java_name, field_line)
}

pub(super) fn insert_entity_member(
    source: &str,
    entity_java_name: &str,
    member: &str,
) -> Result<String> {
    let mut inside_target = false;
    let mut depth = 0usize;
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if !inside_target && declaration.starts_with("entity ") && declaration.ends_with('{') {
            let name = declaration["entity ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            inside_target = name == entity_java_name;
            if inside_target {
                depth = 1;
            }
        } else if inside_target && declaration.ends_with('{') {
            depth += 1;
        } else if inside_target && declaration == "}" {
            if depth == 1 {
                let mut next = source.to_string();
                next.insert_str(byte_offset, &format!("{member}\n"));
                return Ok(next);
            }
            depth -= 1;
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL body for entity `{entity_java_name}`\n       fix: keep the entity as a top-level `entity Name {{ ... }}` block and retry"
    )))
}

pub(crate) fn remove_capability(
    source: &str,
    capability_kind: &str,
    capability_id: &str,
) -> Result<String> {
    let explicit_id = format!("@id({capability_id})");
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if let Some(rest) = declaration.strip_prefix("capability ") {
            let kind = rest.split_whitespace().next().unwrap_or_default();
            if kind == capability_kind
                && (declaration.contains(&explicit_id) || !declaration.contains("@id("))
            {
                let mut next = source.to_string();
                next.replace_range(byte_offset..byte_offset + line.len(), "");
                return Ok(next);
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL declaration for capability `{capability_kind}`\n       fix: keep it as a top-level `capability {capability_kind}` line and retry"
    )))
}

pub(crate) fn remove_dependency(
    source: &str,
    coordinate: &str,
    dependency_id: &str,
) -> Result<String> {
    remove_top_level_line(source, "dependency ", coordinate, dependency_id)
}

pub(crate) fn remove_setting(source: &str, key: &str, setting_id: &str) -> Result<String> {
    remove_top_level_line(source, "setting ", key, setting_id)
}

pub(crate) fn remove_unit(
    source: &str,
    kind: &str,
    java_stem: &str,
    unit_id: &str,
) -> Result<String> {
    remove_top_level_line(source, &format!("{kind} "), java_stem, unit_id)
}

pub(crate) fn set_entity_active(
    source: &str,
    entity_java_name: &str,
    active: bool,
) -> Result<String> {
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if declaration.starts_with("entity ") && declaration.ends_with('{') {
            let name = declaration["entity ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if name == entity_java_name {
                let inactive = declaration
                    .split_whitespace()
                    .any(|word| word == "@inactive");
                if inactive == !active {
                    return Ok(source.to_string());
                }
                let mut rewritten = line.to_string();
                if active {
                    rewritten = rewritten.replacen(" @inactive", "", 1);
                } else {
                    let brace = rewritten.find('{').ok_or_else(|| {
                        Failure::Told(format!(
                            "the JDL entity `{entity_java_name}` has no opening brace\n       fix: keep the entity header as `entity {entity_java_name} {{` and retry"
                        ))
                    })?;
                    rewritten.insert_str(brace, "@inactive ");
                }
                let mut next = source.to_string();
                next.replace_range(byte_offset..byte_offset + line.len(), &rewritten);
                return Ok(next);
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL entity `{entity_java_name}`\n       fix: keep it as a top-level `entity {entity_java_name} {{ ... }}` block and retry"
    )))
}

pub(crate) fn remove_entity(
    source: &str,
    entity_java_name: &str,
    entity_id: &str,
) -> Result<String> {
    remove_block(source, &["entity ", "enum "], entity_java_name, entity_id)
}

pub(crate) fn remove_operation(
    source: &str,
    operation_java_name: &str,
    operation_id: &str,
) -> Result<String> {
    remove_block(
        source,
        &["command ", "query ", "transition ", "event "],
        operation_java_name,
        operation_id,
    )
}

fn remove_block(source: &str, prefixes: &[&str], name: &str, stable_id: &str) -> Result<String> {
    let explicit_id = format!("@id({stable_id})");
    let mut byte_offset = 0;
    let mut start = None;
    let mut depth = 0usize;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if start.is_none() {
            let matches = prefixes.iter().any(|prefix| {
                declaration
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.split([' ', '(']).next().unwrap_or_default() == name)
            });
            if matches
                && declaration.ends_with('{')
                && (declaration.contains(&explicit_id) || !declaration.contains("@id("))
            {
                start = Some(byte_offset);
                depth = 1;
            }
        } else if let Some(block_start) = start {
            if declaration.ends_with('{') {
                depth += 1;
            } else if declaration == "}" {
                depth -= 1;
                if depth == 0 {
                    let mut next = source.to_string();
                    next.replace_range(block_start..byte_offset + line.len(), "");
                    return Ok(next);
                }
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL block `{name}` with identity `{stable_id}`\n       fix: keep the declaration as one brace-delimited JDL block with its `@id(...)` annotation and retry"
    )))
}

fn remove_top_level_line(
    source: &str,
    prefix: &str,
    name: &str,
    stable_id: &str,
) -> Result<String> {
    let explicit_id = format!("@id({stable_id})");
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if let Some(rest) = declaration.strip_prefix(prefix) {
            let candidate = rest.split_whitespace().next().unwrap_or_default();
            if candidate == name
                && (declaration.contains(&explicit_id) || !declaration.contains("@id("))
            {
                let mut next = source.to_string();
                next.replace_range(byte_offset..byte_offset + line.len(), "");
                return Ok(next);
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL declaration `{prefix}{name}`\n       fix: keep it as one top-level JDL line and retry"
    )))
}

pub(crate) fn rename_entity(
    source: &str,
    current_java_name: &str,
    next_java_name: &str,
    stable_label: &str,
) -> Result<String> {
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let code = line.split("//").next().unwrap_or_default();
        let declaration = code.trim();
        let keyword = if declaration.starts_with("entity ") {
            "entity "
        } else if declaration.starts_with("enum ") {
            "enum "
        } else {
            byte_offset += line.len();
            continue;
        };
        let name = declaration[keyword.len()..]
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if name != current_java_name {
            byte_offset += line.len();
            continue;
        }

        let declaration_at = line
            .find(declaration)
            .expect("trimmed text belongs to line");
        let name_at = declaration_at + keyword.len();
        let mut rewritten = line.to_string();
        rewritten.replace_range(name_at..name_at + name.len(), next_java_name);
        if !declaration.contains("@as(") && java_to_label(next_java_name) != stable_label {
            let brace = rewritten.find('{').ok_or_else(|| {
                Failure::Told(format!(
                    "the JDL declaration for `{current_java_name}` has no opening brace\n       fix: keep it as `{keyword}{current_java_name} {{` and retry"
                ))
            })?;
            rewritten.insert_str(brace, &format!("@as({stable_label}) "));
        }
        let mut next = source.to_string();
        next.replace_range(byte_offset..byte_offset + line.len(), &rewritten);
        return Ok(next);
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL declaration for entity `{current_java_name}`\n       fix: keep it as a top-level `entity {current_java_name} {{ ... }}` block and retry"
    )))
}
