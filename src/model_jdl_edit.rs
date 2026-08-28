//! Lossless edits to compact field declarations in the JDL authoring source.

use crate::model_resource::java_to_label;
use jails_support::{Failure, Result};

pub(crate) fn rename_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    next_name: &str,
    stable_label: &str,
    next_column: Option<&str>,
) -> Result<String> {
    rewrite_field(source, entity, field, field_id, |line, declaration| {
        let declaration_at = line
            .find(declaration)
            .expect("trimmed declaration belongs to its line");
        let name = declaration
            .split_once(':')
            .expect("field has colon")
            .0
            .trim();
        let mut rewritten = line.to_string();
        rewritten.replace_range(declaration_at..declaration_at + name.len(), next_name);
        if !declaration.contains("@as(") && java_to_label(next_name) != stable_label {
            rewritten = set_annotation(&rewritten, "as", stable_label);
        }
        if let Some(column) = next_column {
            rewritten = set_annotation(&rewritten, "column", column);
        }
        Ok(rewritten)
    })
}

pub(crate) fn set_field_type(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    next_type: &str,
) -> Result<String> {
    rewrite_field(source, entity, field, field_id, |line, declaration| {
        replace_type_token(line, declaration, |token| {
            let suffix = token
                .chars()
                .last()
                .filter(|character| matches!(character, '!' | '?'))
                .map(|character| character.to_string())
                .unwrap_or_default();
            format!("{next_type}{suffix}")
        })
    })
}

pub(crate) fn set_field_required(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    required: bool,
) -> Result<String> {
    rewrite_field(source, entity, field, field_id, |line, declaration| {
        replace_type_token(line, declaration, |token| {
            let base = token.trim_end_matches(['!', '?']);
            format!("{base}{}", if required { "" } else { "?" })
        })
    })
}

pub(crate) fn remove_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
) -> Result<String> {
    rewrite_field(source, entity, field, field_id, |_line, _declaration| {
        Ok(String::new())
    })
}

fn rewrite_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    edit: impl FnOnce(&str, &str) -> Result<String>,
) -> Result<String> {
    let explicit_id = format!("@id({field_id})");
    let mut inside = false;
    let mut depth = 0usize;
    let mut byte_offset = 0;
    let mut edit = Some(edit);
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if !inside && declaration.starts_with("entity ") && declaration.ends_with('{') {
            let candidate = declaration["entity ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            inside = candidate == entity;
            if inside {
                depth = 1;
            }
        } else if inside && declaration.ends_with('{') {
            depth += 1;
        } else if inside && declaration == "}" {
            if depth == 1 {
                break;
            }
            depth -= 1;
        } else if inside && depth == 1 {
            let candidate = declaration
                .split_once(':')
                .map(|(name, _)| name.trim())
                .unwrap_or_default();
            if candidate == field
                && (declaration.contains(&explicit_id) || !declaration.contains("@id("))
            {
                let rewritten = edit.take().expect("field edit runs once")(line, declaration)?;
                let mut next = source.to_string();
                next.replace_range(byte_offset..byte_offset + line.len(), &rewritten);
                return Ok(next);
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find JDL field `{entity}.{field}` with identity `{field_id}`\n       fix: keep it as one compact field line inside `entity {entity} {{ ... }}` and retry"
    )))
}

fn replace_type_token(
    line: &str,
    declaration: &str,
    replacement: impl FnOnce(&str) -> String,
) -> Result<String> {
    let declaration_at = line
        .find(declaration)
        .expect("trimmed declaration belongs to its line");
    let colon = declaration.find(':').expect("field has colon");
    let after_colon = &declaration[colon + 1..];
    let leading = after_colon.len() - after_colon.trim_start().len();
    let token_start = declaration_at + colon + 1 + leading;
    let token = after_colon.split_whitespace().next().ok_or_else(|| {
        Failure::Told(
            "the JDL field has no type token\n       fix: write the field as `name: type` and retry"
                .to_string(),
        )
    })?;
    let mut rewritten = line.to_string();
    rewritten.replace_range(token_start..token_start + token.len(), &replacement(token));
    Ok(rewritten)
}

fn set_annotation(line: &str, name: &str, value: &str) -> String {
    let prefix = format!("@{name}(");
    let mut rewritten = line.to_string();
    if let Some(start) = rewritten.find(&prefix) {
        let value_start = start + prefix.len();
        if let Some(end) = rewritten[value_start..].find(')') {
            rewritten.replace_range(value_start..value_start + end, value);
            return rewritten;
        }
    }
    let code_end = rewritten
        .find("//")
        .unwrap_or_else(|| rewritten.trim_end_matches(['\r', '\n']).trim_end().len());
    let insertion = rewritten[..code_end].trim_end().len();
    rewritten.insert_str(insertion, &format!(" @{name}({value})"));
    rewritten
}
