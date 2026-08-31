//! Lossless edits shared by the legacy and JDL v1 familiar frontends.

use crate::model_resource::java_to_label;
use jails_support::{Failure, Result};

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
    if is_v1_source(source) {
        let declaration = member.trim_start();
        let kind = declaration
            .split_whitespace()
            .next()
            .filter(|word| {
                matches!(
                    *word,
                    "use"
                        | "table"
                        | "pk"
                        | "unique"
                        | "index"
                        | "relation"
                        | "command"
                        | "query"
                        | "transition"
                        | "event"
                )
            })
            .unwrap_or("field");
        return jails_model::insert_jdl_entity_member(source, entity_java_name, kind, member)
            .map_err(jdl_edit_failure);
    }
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
    capability_label: &str,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(source, &["cap"], capability_label)
            .map_err(jdl_edit_failure);
    }
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
    dependency_label: &str,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(source, &["dep"], dependency_label)
            .map_err(jdl_edit_failure);
    }
    remove_top_level_line(source, "dependency ", coordinate, dependency_id)
}

pub(crate) fn remove_setting(
    source: &str,
    key: &str,
    setting_id: &str,
    setting_label: &str,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(source, &["prop"], setting_label)
            .map_err(jdl_edit_failure);
    }
    remove_top_level_line(source, "setting ", key, setting_id)
}

pub(crate) fn remove_unit(
    source: &str,
    kind: &str,
    java_stem: &str,
    unit_id: &str,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(source, &["component"], java_stem)
            .map_err(jdl_edit_failure);
    }
    remove_top_level_line(source, &format!("{kind} "), java_stem, unit_id)
}

pub(crate) fn set_entity_active(
    source: &str,
    entity_java_name: &str,
    active: bool,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::set_jdl_entity_attribute(source, entity_java_name, "retired", !active)
            .map_err(jdl_edit_failure);
    }
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
                if inactive != active {
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
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(source, &["entity", "enum"], entity_java_name)
            .map_err(jdl_edit_failure);
    }
    remove_block(source, &["entity ", "enum "], entity_java_name, entity_id)
}

pub(crate) fn remove_operation(
    source: &str,
    operation_java_name: &str,
    operation_id: &str,
) -> Result<String> {
    if is_v1_source(source) {
        return jails_model::remove_jdl_declaration(
            source,
            &["command", "query", "transition", "event"],
            operation_java_name,
        )
        .map_err(jdl_edit_failure);
    }
    remove_block(
        source,
        &["command ", "query ", "transition ", "event "],
        operation_java_name,
        operation_id,
    )
}

pub(crate) fn is_v1_source(source: &str) -> bool {
    source
        .lines()
        .find_map(|line| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with("//")).then_some(line)
        })
        .is_some_and(|line| line.split_whitespace().next() == Some("jdl"))
}

pub(crate) fn jdl_edit_failure(diagnostics: jails_model::Diagnostics) -> Failure {
    Failure::Told(diagnostics.to_string().trim_end().to_string())
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
    stable_id: &str,
    pinned_table: Option<&str>,
    pinned_route: Option<(&str, &str)>,
) -> Result<String> {
    if is_v1_source(source) {
        let mut renamed = jails_model::rename_jdl_declaration(
            source,
            &["entity", "enum"],
            current_java_name,
            next_java_name,
            stable_id,
        )
        .map_err(jdl_edit_failure)?;
        if let Some(table) = pinned_table {
            let cst = jails_model::parse_jdl_cst(&renamed).map_err(jdl_edit_failure)?;
            let owner = java_to_label(next_java_name);
            if !cst
                .members
                .iter()
                .any(|member| member.owner == owner && member.kind == "table")
            {
                let table = serde_json::to_string(table).map_err(|error| {
                    Failure::Told(format!("could not quote preserved table name: {error}"))
                })?;
                renamed = jails_model::insert_jdl_entity_member(
                    &renamed,
                    next_java_name,
                    "table",
                    &format!("  table {table}"),
                )
                .map_err(jdl_edit_failure)?;
            }
        }
        // **The route does not move with the name unless asked.** Every
        // other derived name here is jails' own; a route has callers, and a
        // rename that quietly answers 404 where it answered 200 yesterday is
        // the one convention change a compiler must not make silently. See
        // `set_jdl_projection_path`.
        if let Some((projection, route)) = pinned_route {
            renamed =
                jails_model::set_jdl_projection_path(&renamed, next_java_name, projection, route)
                    .map_err(jdl_edit_failure)?;
        }
        return Ok(renamed);
    }
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
