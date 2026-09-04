//! Initial SQLite schema lowering.
//!
//! The Java runner is an ejectable capability pack. The SQL is not: once
//! written it joins append-only migration history and survives capability
//! removal, Java ejection, and later compilation.

use jails_contracts::RenderedMigration;
use jails_model::{AppModel, StableId};
use std::collections::BTreeSet;

const FIRST_MIGRATION: &str = "-- Applied once, in filename order, by Migrations.applyAll.\ncreate table if not exists item (\n    id integer primary key autoincrement,\n    name text not null,\n    qty integer not null default 0\n);\n";

pub(super) fn derive(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    if next.project.dialect == "postgresql" || crate::emit_sql::has_database(next) {
        return Vec::new();
    }
    let next_id = capability_id(next);
    let was_present = accepted.and_then(capability_id).is_some();
    next_id
        .filter(|_| !was_present)
        .map(|id| RenderedMigration {
            logical_name: "sqlite_init".to_string(),
            bytes: FIRST_MIGRATION.as_bytes().to_vec(),
            semantic_ids: BTreeSet::from([id.to_string()]),
        })
        .into_iter()
        .collect()
}

fn capability_id(model: &AppModel) -> Option<&str> {
    model
        .capabilities
        .values()
        .find(|capability| capability.kind == "sqlite")
        .map(|capability| capability.id.as_str())
}
