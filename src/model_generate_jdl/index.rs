//! Lossless JDL edits for nested composite and ordered indexes.

use jails_support::Result;

pub(crate) fn insert(source: &str, entity_java_name: &str, index_line: &str) -> Result<String> {
    super::insert_entity_member(source, entity_java_name, index_line)
}

pub(crate) fn remove(source: &str, entity_java_name: &str, index_id: &str) -> Result<String> {
    jails_model::remove_jdl_entity_member(
        source,
        entity_java_name,
        &["index"],
        None,
        Some(index_id),
    )
    .map_err(super::jdl_edit_failure)
}
