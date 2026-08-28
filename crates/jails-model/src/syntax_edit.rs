//! Lossless, bounded edits to the human-owned model syntax.
//!
//! The semantic linker remains authoritative. These helpers only remove a
//! complete canonical declaration block and preserve every unrelated byte.

/// Change only the Java projection of an entity while keeping its stable
/// model label (and therefore its default SQL table) unchanged.
pub fn set_entity_java_name(source: &str, label: &str, java_name: &str) -> Result<String, String> {
    set_table_assignment(
        source,
        &format!("entities.{label}"),
        "java_name",
        &quoted(java_name),
        &format!("canonical entity table `[entities.{label}]` was not found"),
    )
}

pub fn set_entity_active(source: &str, label: &str, active: bool) -> Result<String, String> {
    set_table_assignment(
        source,
        &format!("entities.{label}"),
        "active",
        if active { "true" } else { "false" },
        &format!("canonical entity table `[entities.{label}]` was not found"),
    )
}

pub fn set_field_java_name(
    source: &str,
    entity: &str,
    field: &str,
    java_name: &str,
) -> Result<String, String> {
    set_field_assignment(source, entity, field, "java_name", &quoted(java_name))
}

pub fn set_field_column(
    source: &str,
    entity: &str,
    field: &str,
    column: &str,
) -> Result<String, String> {
    set_field_assignment(source, entity, field, "column", &quoted(column))
}

pub fn set_field_type(
    source: &str,
    entity: &str,
    field: &str,
    type_name: &str,
) -> Result<String, String> {
    set_field_assignment(source, entity, field, "type", &quoted(type_name))
}

pub fn set_field_required(
    source: &str,
    entity: &str,
    field: &str,
    required: bool,
) -> Result<String, String> {
    set_field_assignment(
        source,
        entity,
        field,
        "required",
        if required { "true" } else { "false" },
    )
}

pub fn remove_field_declaration(source: &str, entity: &str, field: &str) -> Result<String, String> {
    let target = format!("entities.{entity}.fields.{field}");
    remove_declarations(
        source,
        |header| header == target,
        &format!("canonical field table `[{target}]` was not found"),
    )
}

pub fn remove_index_declaration(source: &str, entity: &str, index: &str) -> Result<String, String> {
    let target = format!("entities.{entity}.indexes.{index}");
    remove_declarations(
        source,
        |header| header == target,
        &format!("canonical index table `[{target}]` was not found"),
    )
}

fn set_field_assignment(
    source: &str,
    entity: &str,
    field: &str,
    key: &str,
    value: &str,
) -> Result<String, String> {
    let target = format!("entities.{entity}.fields.{field}");
    set_table_assignment(
        source,
        &target,
        key,
        value,
        &format!("canonical field table `[{target}]` was not found"),
    )
}

fn set_table_assignment(
    source: &str,
    target: &str,
    key: &str,
    value: &str,
    missing: &str,
) -> Result<String, String> {
    let mut table_start = None;
    let mut table_end = source.len();
    let mut offset = 0_usize;

    for line in source.split_inclusive('\n') {
        if let Some(header) = table_header(line) {
            if table_start.is_some() {
                table_end = offset;
                break;
            }
            if header == target {
                table_start = Some(offset + line.len());
            }
        }
        offset += line.len();
    }
    let Some(start) = table_start else {
        return Err(format!(
            "{missing}\n       fix: use a canonical bare table header, then retry"
        ));
    };
    let table = &source[start..table_end];
    let mut relative = 0_usize;
    for line in table.split_inclusive('\n') {
        if let Some(replacement) = replace_assignment_value(line, key, value) {
            let assignment_start = start + relative;
            let assignment_end = assignment_start + line.len();
            let mut output = String::with_capacity(source.len() + replacement.len());
            output.push_str(&source[..assignment_start]);
            output.push_str(&replacement);
            output.push_str(&source[assignment_end..]);
            return Ok(output);
        }
        relative += line.len();
    }

    let mut output = String::with_capacity(source.len() + value.len() + key.len() + 8);
    output.push_str(&source[..start]);
    output.push_str(&format!("{key} = {value}\n"));
    output.push_str(&source[start..]);
    Ok(output)
}

fn replace_assignment_value(line: &str, key: &str, rendered: &str) -> Option<String> {
    let newline = if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let body = &line[..line.len() - newline.len()];
    let comment = body.find('#').unwrap_or(body.len());
    let code = &body[..comment];
    let equals = code.find('=')?;
    if code[..equals].trim() != key {
        return None;
    }
    let after_equals = &code[equals + 1..];
    let leading = after_equals.len() - after_equals.trim_start().len();
    let trailing = after_equals.len() - after_equals.trim_end().len();
    let value_start = equals + 1 + leading;
    let value_end = code.len() - trailing;
    let mut output = String::with_capacity(line.len() + rendered.len());
    output.push_str(&body[..value_start]);
    output.push_str(rendered);
    output.push_str(&body[value_end..]);
    output.push_str(newline);
    Some(output)
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn remove_entity_declaration(source: &str, label: &str) -> Result<String, String> {
    let entity = format!("entities.{label}");
    let fields = format!("{entity}.fields.");
    remove_declarations(
        source,
        |header| header == entity || header.starts_with(&fields),
        &format!("canonical entity table `[entities.{label}]` was not found"),
    )
}

pub fn remove_operation_declaration(source: &str, label: &str) -> Result<String, String> {
    let operation = format!("operations.{label}");
    remove_declarations(
        source,
        |header| header == operation,
        &format!("canonical operation table `[operations.{label}]` was not found"),
    )
}

pub fn remove_unit_declaration(source: &str, label: &str) -> Result<String, String> {
    let unit = format!("units.{label}");
    remove_declarations(
        source,
        |header| header == unit,
        &format!("canonical source unit table `[units.{label}]` was not found"),
    )
}

pub fn remove_capability_declaration(source: &str, label: &str) -> Result<String, String> {
    let capability = format!("capabilities.{label}");
    remove_declarations(
        source,
        |header| header == capability,
        &format!("canonical capability table `[capabilities.{label}]` was not found"),
    )
}

pub fn remove_dependency_declaration(source: &str, label: &str) -> Result<String, String> {
    let dependency = format!("dependencies.{label}");
    remove_declarations(
        source,
        |header| header == dependency,
        &format!("canonical dependency table `[dependencies.{label}]` was not found"),
    )
}

pub fn remove_setting_declaration(source: &str, label: &str) -> Result<String, String> {
    let setting = format!("settings.{label}");
    remove_declarations(
        source,
        |header| header == setting,
        &format!("canonical setting table `[settings.{label}]` was not found"),
    )
}

fn remove_declarations(
    source: &str,
    target: impl Fn(&str) -> bool,
    missing: &str,
) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    let mut skipping = false;
    let mut found = false;
    let mut pending_trivia = String::new();

    for line in source.split_inclusive('\n') {
        if let Some(header) = table_header(line) {
            let is_target = target(header);
            found |= is_target;
            if is_target {
                pending_trivia.clear();
            } else if skipping {
                output.push_str(&pending_trivia);
                pending_trivia.clear();
            }
            skipping = is_target;
        }
        if !skipping {
            output.push_str(line);
        } else if table_header(line).is_none()
            && (line.trim().is_empty() || line.trim_start().starts_with('#'))
        {
            pending_trivia.push_str(line);
        } else {
            pending_trivia.clear();
        }
    }
    if skipping {
        output.push_str(&pending_trivia);
    }
    if !found {
        return Err(format!(
            "{missing}\n       fix: use canonical bare table headers, then retry"
        ));
    }
    while output.ends_with("\n\n\n") {
        output.pop();
    }
    Ok(output)
}

fn table_header(line: &str) -> Option<&str> {
    let candidate = line
        .split_once('#')
        .map_or(line, |(before, _)| before)
        .trim();
    if candidate.starts_with("[[") || !candidate.starts_with('[') || !candidate.ends_with(']') {
        return None;
    }
    Some(&candidate[1..candidate.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_an_entity_preserves_unrelated_model_bytes() {
        let source = "schema = \"jails.model.v1\"\n\n[entities.note]\nid = \"ent_note\"\n\n[entities.note.fields.id]\nid = \"fld_note_id\"\n\n# reader comment\n[operations.ping]\nkind = \"event\"\nid = \"op_ping\"\n";
        let edited = remove_entity_declaration(source, "note").unwrap();
        assert!(!edited.contains("[entities.note]"));
        assert!(!edited.contains("fld_note_id"));
        assert!(edited.contains("# reader comment\n[operations.ping]"));
        assert!(edited.starts_with("schema = \"jails.model.v1\""));
    }

    #[test]
    fn removing_an_operation_preserves_adjacent_operations() {
        let source = "[operations.first]\nkind = \"event\"\nid = \"op_first\"\n\n[operations.second]\nkind = \"event\"\nid = \"op_second\"\n";
        let edited = remove_operation_declaration(source, "first").unwrap();
        assert!(!edited.contains("operations.first"));
        assert!(edited.contains("[operations.second]"));
        assert!(edited.contains("op_second"));
    }

    #[test]
    fn setting_an_entity_java_name_preserves_every_other_byte() {
        let source = "[entities.task]\nid = \"ent_task\"\njava_name  =  \"Task\"  # public type\nfacets = [\"record\"]\n\n[entities.task.fields.title]\nid = \"fld_task_title\"\ntype = \"string\"\n";
        let edited = set_entity_java_name(source, "task", "WorkItem").unwrap();
        assert_eq!(edited, source.replace("\"Task\"", "\"WorkItem\""));
    }

    #[test]
    fn setting_an_entity_java_name_inserts_only_the_projection() {
        let source = "[entities.task]\nid = \"ent_task\"\nfacets = [\"record\"]\n";
        let edited = set_entity_java_name(source, "task", "WorkItem").unwrap();
        assert_eq!(
            edited,
            "[entities.task]\njava_name = \"WorkItem\"\nid = \"ent_task\"\nfacets = [\"record\"]\n"
        );
    }

    #[test]
    fn field_edits_preserve_comments_spacing_and_stable_identity() {
        let source = "[entities.task.fields.due_at]\nid = \"fld_task_due_at\"\ntype  =  \"instant\" # clock\nrequired = true\njava_name = \"dueAt\"\ncolumn = \"due_at\"\n";
        let renamed = set_field_java_name(source, "task", "due_at", "deadline").unwrap();
        let renamed = set_field_column(&renamed, "task", "due_at", "deadline").unwrap();
        let typed = set_field_type(&renamed, "task", "due_at", "datetime").unwrap();
        let nullable = set_field_required(&typed, "task", "due_at", false).unwrap();
        assert!(nullable.contains("id = \"fld_task_due_at\""));
        assert!(nullable.contains("type  =  \"datetime\" # clock"));
        assert!(nullable.contains("required = false"));
        assert!(nullable.contains("java_name = \"deadline\""));
        assert!(nullable.contains("column = \"deadline\""));
    }

    #[test]
    fn removing_one_field_keeps_the_entity_and_adjacent_fields_byte_for_byte() {
        let source = "[entities.task]\nid = \"ent_task\"\n\n[entities.task.fields.title]\nid = \"fld_task_title\"\ntype = \"string\"\n\n# keep this\n[entities.task.fields.done]\nid = \"fld_task_done\"\ntype = \"boolean\"\n";
        let edited = remove_field_declaration(source, "task", "title").unwrap();
        assert_eq!(
            edited,
            "[entities.task]\nid = \"ent_task\"\n\n\n# keep this\n[entities.task.fields.done]\nid = \"fld_task_done\"\ntype = \"boolean\"\n"
        );
    }

    #[test]
    fn removing_one_index_keeps_the_entity_and_adjacent_index_byte_for_byte() {
        let source = "[entities.task]\nid = \"ent_task\"\n\n[entities.task.indexes.by_title]\nid = \"idx_task_title\"\ncolumns = [\"title\"]\n\n# keep this\n[entities.task.indexes.by_done]\nid = \"idx_task_done\"\ncolumns = [\"done\"]\n";
        let edited = remove_index_declaration(source, "task", "by_title").unwrap();
        assert!(!edited.contains("by_title"));
        assert!(edited.contains("# keep this\n[entities.task.indexes.by_done]"));
        assert!(edited.contains("idx_task_done"));
    }
}
