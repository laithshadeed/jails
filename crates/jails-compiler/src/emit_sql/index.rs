//! Stable-ID index diffing and forward PostgreSQL statements.

use crate::CompileError;
use jails_model::{Entity, StableId};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn derive_changes(
    old: &Entity,
    current: &Entity,
    removals: &BTreeMap<String, String>,
    statements: &mut Vec<String>,
    semantic_ids: &mut BTreeSet<String>,
    descriptions: &mut Vec<String>,
) -> Result<(), CompileError> {
    for old_index in old.indexes.values() {
        let Some(current_index) = current.indexes.get(&old_index.id) else {
            let Some(confirmed_name) = removals.get(old_index.id.as_str()) else {
                return Err(CompileError::new(format!(
                    "accepted index `{}` was removed without a drop policy\n       fix: use `resource index remove {} {} --confirm-index {}`",
                    old_index.sql_name, old.names.java_type, old_index.label, old_index.sql_name
                )));
            };
            if confirmed_name != &old_index.sql_name {
                return Err(CompileError::new(format!(
                    "confirmed index `{confirmed_name}` is not accepted index `{}`\n       fix: pass `--confirm-index {}` exactly",
                    old_index.sql_name, old_index.sql_name
                )));
            }
            statements.push(format!("drop index {};", old_index.sql_name));
            semantic_ids.extend([
                old.id.as_str().to_string(),
                old_index.id.as_str().to_string(),
            ]);
            descriptions.push(format!("drop_{}", old_index.sql_name));
            continue;
        };
        if old_index != current_index {
            return Err(CompileError::new(format!(
                "accepted index `{}` changed without an evolution policy\n       fix: add a replacement index before retiring this one",
                old_index.sql_name
            )));
        }
    }
    Ok(())
}
