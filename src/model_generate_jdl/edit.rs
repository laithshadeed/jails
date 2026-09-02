//! Lossless edits to the JDL v1 authoring source, shared by the familiar
//! frontends.
//!
//! Every function here is a CST edit and nothing else. The pre-v1 draft does
//! not accept edits -- `model_command::read_source` refuses one and names
//! `jails model upgrade --to 1` -- so there is no line-by-line scanner beside
//! these, and no parameters for finding a line by hand: the CST is indexed by
//! declaration rather than searched.

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
    jails_model::insert_jdl_entity_member(source, entity_java_name, kind, member)
        .map_err(jdl_edit_failure)
}

pub(crate) fn remove_capability(source: &str, capability_label: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(source, &["cap"], capability_label)
        .map_err(jdl_edit_failure)
}

pub(crate) fn remove_dependency(source: &str, dependency_label: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(source, &["dep"], dependency_label)
        .map_err(jdl_edit_failure)
}

pub(crate) fn remove_setting(source: &str, setting_label: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(source, &["prop"], setting_label).map_err(jdl_edit_failure)
}

pub(crate) fn remove_unit(source: &str, java_stem: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(source, &["component"], java_stem).map_err(jdl_edit_failure)
}

pub(crate) fn set_entity_active(
    source: &str,
    entity_java_name: &str,
    active: bool,
) -> Result<String> {
    jails_model::set_jdl_entity_attribute(source, entity_java_name, "retired", !active)
        .map_err(jdl_edit_failure)
}

pub(crate) fn remove_entity(source: &str, entity_java_name: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(source, &["entity", "enum"], entity_java_name)
        .map_err(jdl_edit_failure)
}

pub(crate) fn remove_operation(source: &str, operation_java_name: &str) -> Result<String> {
    jails_model::remove_jdl_declaration(
        source,
        &["command", "query", "transition", "event"],
        operation_java_name,
    )
    .map_err(jdl_edit_failure)
}

pub(crate) fn jdl_edit_failure(diagnostics: jails_model::Diagnostics) -> Failure {
    Failure::Told(diagnostics.to_string().trim_end().to_string())
}

pub(crate) fn rename_entity(
    source: &str,
    current_java_name: &str,
    next_java_name: &str,
    stable_id: &str,
    pinned_table: Option<&str>,
    pinned_route: Option<(&str, &str)>,
) -> Result<String> {
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
        let owner = jails_model::field_syntax::java_to_label(next_java_name);
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
    // **The route does not move with the name unless asked.** Every other
    // derived name here is jails' own; a route has callers, and a rename that
    // quietly answers 404 where it answered 200 yesterday is the one
    // convention change a compiler must not make silently. See
    // `set_jdl_projection_path`.
    if let Some((projection, route)) = pinned_route {
        renamed = jails_model::set_jdl_projection_path(&renamed, next_java_name, projection, route)
            .map_err(jdl_edit_failure)?;
    }
    Ok(renamed)
}
