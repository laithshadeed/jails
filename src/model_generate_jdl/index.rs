//! Lossless JDL edits for nested composite and ordered indexes.

use jails_support::{Failure, Result};

pub(crate) fn insert(source: &str, entity_java_name: &str, index_line: &str) -> Result<String> {
    super::insert_entity_member(source, entity_java_name, index_line)
}

pub(crate) fn remove(source: &str, entity_java_name: &str, index_id: &str) -> Result<String> {
    let explicit_id = format!("@id({index_id})");
    let mut inside_target = false;
    let mut depth = 0usize;
    let mut byte_offset = 0usize;
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
                break;
            }
            depth -= 1;
        } else if inside_target
            && depth == 1
            && declaration.starts_with("index ")
            && declaration.contains(&explicit_id)
        {
            let mut next = source.to_string();
            next.replace_range(byte_offset..byte_offset + line.len(), "");
            return Ok(next);
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL declaration for index `{index_id}` on `{entity_java_name}`\n       fix: keep the index as one `index (...) @id({index_id})` line and retry"
    )))
}
