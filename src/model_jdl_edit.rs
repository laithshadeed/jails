//! Lossless edits to compact field declarations in the JDL authoring source.

use jails_model::field_syntax::java_to_label;
use jails_support::{Failure, Result};

pub(crate) fn rename_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
    next_name: &str,
    next_column: Option<&str>,
) -> Result<String> {
    let intermediate = rewrite_field(source, entity, field, field_id, |line, declaration| {
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
    })?;
    cascade_field_rename(&intermediate, entity, field, next_name)
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
        let rewritten = replace_type_token(line, declaration, |token| {
            let base = token.trim_end_matches(['!', '?']);
            format!("{base}{}", if required { "" } else { "?" })
        })?;
        // **`@notBlank` cannot survive the relaxation.** A non-blank field
        // cannot be optional -- `model-non-blank-required` refuses the whole
        // model -- so relaxing one has exactly one valid outcome. v1 states
        // the two facts apart, so the editor drops the annotation out loud;
        // otherwise `title: string? @notBlank` is written and the very next
        // read of the model refuses, naming a contradiction the reader did
        // not write.
        Ok(match required {
            true => rewritten,
            false => rewritten.replace(" @notBlank", ""),
        })
    })
}

pub(crate) fn remove_field(
    source: &str,
    entity: &str,
    field: &str,
    field_id: &str,
) -> Result<String> {
    let intermediate = rewrite_field(source, entity, field, field_id, |_line, _declaration| {
        Ok(String::new())
    })?;
    cascade_field_remove(&intermediate, entity, field)
}

fn cascade_field_rename(
    source: &str,
    entity: &str,
    field: &str,
    next_name: &str,
) -> Result<String> {
    let cst =
        jails_model::parse_jdl_cst(source).map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    let owner = java_to_label(entity);
    let old_label = java_to_label(field);
    let next_label = java_to_label(next_name);

    let mut edits: Vec<(jails_model::JdlSpan, String)> = Vec::new();

    for member in &cst.members {
        if member.owner == owner && matches!(member.kind.as_str(), "use" | "index" | "unique") {
            let text = cst.member_text(member);
            let mut rewritten = replace_identifier(text, field, next_name);
            if field != old_label {
                rewritten = replace_identifier(&rewritten, &old_label, &next_label);
            }
            if rewritten != text {
                edits.push((member.span, rewritten));
            }
        }
    }

    for decl in &cst.declarations {
        if decl.kind == "use" {
            let text = &source[decl.span.start..decl.span.end];
            if target_matches_entity(text, entity, &owner) {
                let mut rewritten = replace_identifier(text, field, next_name);
                if field != old_label {
                    rewritten = replace_identifier(&rewritten, &old_label, &next_label);
                }
                if rewritten != text {
                    edits.push((decl.span, rewritten));
                }
            }
        }
    }

    apply_edits(source, edits)
}

fn cascade_field_remove(source: &str, entity: &str, field: &str) -> Result<String> {
    let cst =
        jails_model::parse_jdl_cst(source).map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    let owner = java_to_label(entity);

    let mut edits: Vec<(jails_model::JdlSpan, String)> = Vec::new();

    for member in &cst.members {
        if member.owner == owner {
            if member.kind == "use" {
                let text = cst.member_text(member);
                if let Some(rewritten) = remove_field_from_use_line(text, field) {
                    edits.push((member.span, rewritten));
                }
            } else if matches!(member.kind.as_str(), "index" | "unique") {
                let text = cst.member_text(member);
                if let Some(rewritten) = remove_field_from_constraint_line(text, field) {
                    edits.push((member.span, rewritten));
                }
            }
        }
    }

    for decl in &cst.declarations {
        if decl.kind == "use" {
            let text = &source[decl.span.start..decl.span.end];
            if target_matches_entity(text, entity, &owner)
                && let Some(rewritten) = remove_field_from_use_line(text, field)
            {
                edits.push((decl.span, rewritten));
            }
        }
    }

    apply_edits(source, edits)
}

fn replace_identifier(text: &str, old_ident: &str, new_ident: &str) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    while let Some(index) = text[cursor..].find(old_ident) {
        let abs_index = cursor + index;
        let before_ok = abs_index == 0 || {
            let prev_char = text[..abs_index].chars().last().unwrap();
            !prev_char.is_ascii_alphanumeric() && prev_char != '_'
        };
        let after_idx = abs_index + old_ident.len();
        let after_ok = after_idx == text.len() || {
            let next_char = text[after_idx..].chars().next().unwrap();
            !next_char.is_ascii_alphanumeric() && next_char != '_'
        };
        if before_ok && after_ok {
            result.push_str(&text[cursor..abs_index]);
            result.push_str(new_ident);
            cursor = after_idx;
        } else {
            result.push_str(&text[cursor..abs_index + old_ident.len()]);
            cursor = abs_index + old_ident.len();
        }
    }
    result.push_str(&text[cursor..]);
    result
}

fn remove_field_from_bracket_list(brackets_content: &str, field: &str) -> (bool, String) {
    let old_label = java_to_label(field);
    let items: Vec<&str> = brackets_content.split(',').collect();
    let mut kept = Vec::new();
    let mut removed = false;
    for item in items {
        let trimmed = item.trim();
        let col_ident = trimmed.split_whitespace().next().unwrap_or_default();
        if col_ident == field || col_ident == old_label || java_to_label(col_ident) == old_label {
            removed = true;
        } else {
            kept.push(trimmed);
        }
    }
    if !removed {
        return (false, brackets_content.to_string());
    }
    (true, kept.join(", "))
}

fn remove_field_from_use_line(line: &str, field: &str) -> Option<String> {
    let search_pos = line.find("search(")?;
    let open_paren = search_pos + "search".len();
    let mut depth = 0;
    let mut close_paren = None;
    for (i, c) in line[open_paren..].char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                close_paren = Some(open_paren + i);
                break;
            }
        }
    }
    let close_paren = close_paren?;
    let search_str = &line[search_pos..=close_paren];
    let bracket_start = search_str.find('[')?;
    let bracket_end = search_str.rfind(']')?;
    if bracket_start >= bracket_end {
        return None;
    }
    let inner = &search_str[bracket_start + 1..bracket_end];
    let (removed, new_inner) = remove_field_from_bracket_list(inner, field);
    if !removed {
        return None;
    }
    if !new_inner.is_empty() {
        let mut new_search = search_str.to_string();
        new_search.replace_range(bracket_start + 1..bracket_end, &new_inner);
        let mut new_line = line.to_string();
        new_line.replace_range(search_pos..=close_paren, &new_search);
        return Some(new_line);
    }
    let before = &line[..search_pos];
    let after = &line[close_paren + 1..];
    if let Some(comma_pos) = before.rfind(',') {
        let mut new_line = before[..comma_pos].to_string();
        new_line.push_str(after);
        Some(new_line)
    } else if let Some(comma_idx) = after.find(',') {
        let mut new_line = before.to_string();
        new_line.push_str(after[comma_idx + 1..].trim_start());
        Some(new_line)
    } else {
        Some(String::new())
    }
}

fn remove_field_from_constraint_line(line: &str, field: &str) -> Option<String> {
    let bracket_start = line.find('[')?;
    let bracket_end = line.rfind(']')?;
    if bracket_start >= bracket_end {
        return None;
    }
    let inner = &line[bracket_start + 1..bracket_end];
    let (removed, new_inner) = remove_field_from_bracket_list(inner, field);
    if !removed {
        return None;
    }
    if !new_inner.is_empty() {
        let mut new_line = line.to_string();
        new_line.replace_range(bracket_start + 1..bracket_end, &new_inner);
        Some(new_line)
    } else {
        Some(String::new())
    }
}

fn target_matches_entity(text: &str, entity: &str, owner: &str) -> bool {
    if let Some((_, after_for)) = text.split_once("for") {
        let targets = after_for.split("except").next().unwrap_or(after_for);
        for target in targets.split(',') {
            let t = target.trim();
            if t == "*" || t == entity || t == owner || java_to_label(t) == owner {
                return true;
            }
        }
    }
    false
}

fn apply_edits(source: &str, mut edits: Vec<(jails_model::JdlSpan, String)>) -> Result<String> {
    if edits.is_empty() {
        return Ok(source.to_string());
    }
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
    let mut current = source.to_string();
    for (span, replacement) in edits {
        current.replace_range(span.start..span.end, &replacement);
    }
    Ok(current)
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

#[cfg(test)]
mod tests {
    use super::*;

    const JDL_SOURCE: &str = r#"jdl 1

app Demo {
  pkg com.example.demo
  java 26
  platform spring
  build maven
  storage postgres
}

entity Item @id(ent_item) {
  use scaffold, factory, seed, dto
  use search(fields: [title, description])
  id: uuid @id(fld_item_id) @pk
  title: string @id(fld_item_title)
  description: string @id(fld_item_description)
}
"#;

    #[test]
    fn rename_field_cascades_to_search_projection() {
        let edited = rename_field(
            JDL_SOURCE,
            "Item",
            "title",
            "fld_item_title",
            "headline",
            None,
        )
        .unwrap();
        assert!(edited.contains("headline: string @id(fld_item_title)"));
        assert!(edited.contains("use search(fields: [headline, description])"));
        assert!(!edited.contains("use search(fields: [title, description])"));
        // Ensure parsing and linking succeeds!
        assert!(jails_model::parse_jdl(&edited).is_ok());
    }

    #[test]
    fn remove_field_cascades_to_search_projection() {
        let edited = remove_field(JDL_SOURCE, "Item", "title", "fld_item_title").unwrap();
        assert!(!edited.contains("title: string"));
        assert!(edited.contains("use search(fields: [description])"));
        assert!(jails_model::parse_jdl(&edited).is_ok());
    }

    #[test]
    fn remove_sole_search_field_drops_search_projection() {
        let single_source = r#"jdl 1

app Demo {
  pkg com.example.demo
  java 26
  platform spring
  build maven
  storage postgres
}

entity Item @id(ent_item) {
  use search(fields: [title])
  id: uuid @id(fld_item_id) @pk
  title: string @id(fld_item_title)
}
"#;
        let edited = remove_field(single_source, "Item", "title", "fld_item_title").unwrap();
        assert!(!edited.contains("title: string"));
        assert!(!edited.contains("use search"));
        assert!(jails_model::parse_jdl(&edited).is_ok());
    }
}
