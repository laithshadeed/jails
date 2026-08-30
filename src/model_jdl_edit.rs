//! Lossless edits to compact field declarations in the JDL authoring source.

use crate::model_resource::java_to_label;
use jails_support::{Failure, Result};

pub(crate) fn rename_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    next_name: &str,
    next_column: Option<&str>,
    pins: &std::collections::BTreeMap<String, String>,
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
        if !declaration.contains("@id(") {
            rewritten = set_annotation(&rewritten, "id", field_id);
        }
        if let Some(column) = next_column {
            rewritten = set_annotation(&rewritten, "map", column);
        }
        Ok(rewritten)
    })
    .and_then(|renamed| rename_in_constraints(&renamed, entity, field, next_name, pins))
}

/// Carry the rename through the entity's index and constraint declarations.
///
/// **A field is named by its declaration and by every bracketed list that
/// covers it**, and the field editor only ever rewrote the first. So renaming
/// a field an `index [...]`, `unique [...]` or `pk [...]` mentioned left the
/// list naming a label that no longer linked -- the rename then refused with
/// "`title` does not name a field", which is true and is not something the
/// author did.
fn rename_in_constraints(
    source: &str,
    entity: &str,
    field: &str,
    next_name: &str,
    pins: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let owner = java_to_label(entity);
    let mut source = source.to_string();
    loop {
        let cst = jails_model::parse_jdl_cst(&source)
            .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
        let Some((span, rewritten)) = cst
            .members
            .iter()
            .filter(|member| {
                member.owner == owner && matches!(member.kind.as_str(), "index" | "unique" | "pk")
            })
            .find_map(|member| {
                let text = cst.member_text(member);
                let (head, tail) = text.split_once('[')?;
                let (list, rest) = tail.split_once(']')?;
                let next = list
                    .split(',')
                    .map(|column| replace_first_word(column, field, next_name))
                    .collect::<Vec<_>>()
                    .join(",");
                if next == list {
                    return None;
                }
                // The SQL name is derived from the labels this list names, so
                // a rename would move it. Pinning what the accepted schema
                // already has keeps the rename a projection change, which is
                // what `--column preserve` means one level down.
                // The member's span includes its newline, so the pin goes
                // before the trailing whitespace rather than after it -- an
                // appended attribute that ate the newline joined this
                // declaration to the next one.
                let mut rewritten = format!("{head}[{next}]{rest}");
                if !rewritten.contains("@map(")
                    && let Some(name) = pins.get(&normalize_columns(list))
                {
                    let body = rewritten.trim_end();
                    let tail = &rewritten[body.len()..];
                    rewritten = format!("{body} @map({name}){tail}");
                }
                Some((member.span, rewritten))
            })
        else {
            return Ok(source);
        };
        source = cst
            .replace_span(span, &rewritten)
            .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    }
}

/// One bracketed list, as a key both the model and the source can produce.
pub(crate) fn normalize_columns(list: &str) -> String {
    list.split(',')
        .map(|column| column.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(",")
}

/// Replace the column name in one entry of a bracketed list, keeping its
/// surrounding whitespace and any `asc`/`desc` ordering.
fn replace_first_word(column: &str, from: &str, to: &str) -> String {
    let trimmed = column.trim();
    let Some(rest) = trimmed.strip_prefix(from) else {
        return column.to_string();
    };
    if rest
        .chars()
        .next()
        .is_some_and(|next| next.is_alphanumeric() || next == '_')
    {
        return column.to_string();
    }
    let at = column
        .find(trimmed)
        .expect("the trim belongs to its column");
    format!("{}{to}{}", &column[..at], &column[at + from.len()..])
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
    let cst =
        jails_model::parse_jdl_cst(source).map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    let owner = java_to_label(entity);
    let explicit_id = format!("@id({field_id})");
    let matches = cst
        .members
        .iter()
        .filter(|member| member.owner == owner && member.kind == "field")
        .filter(|member| member.name.as_deref() == Some(field))
        .filter(|member| {
            let text = cst.member_text(member);
            text.contains(&explicit_id) || !text.contains("@id(")
        })
        .collect::<Vec<_>>();
    let member = match matches.as_slice() {
        [member] => *member,
        [] => {
            return Err(Failure::Told(format!(
                "could not find JDL field `{entity}.{field}` with identity `{field_id}`\n       fix: keep it as a direct field member in the parsed entity block"
            )));
        }
        _ => {
            return Err(Failure::Told(format!(
                "JDL field `{entity}.{field}` is ambiguous\n       fix: give the field one explicit `@id({field_id})`"
            )));
        }
    };
    let line = cst.member_text(member);
    let declaration = line.split("//").next().unwrap_or_default().trim();
    let rewritten = edit(line, declaration)?;
    cst.replace_span(member.span, &rewritten)
        .map_err(crate::model_generate_jdl::jdl_edit_failure)
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
