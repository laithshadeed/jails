//! `component presence <Name>`: who is here, across every node.
//!
//! **PostgreSQL is a precondition, not a preference.** Presence held in one
//! process's memory is correct on one node and wrong on two, with nothing to
//! say which — the application works, and the answer is silently partial. So
//! the store is a table, and a member seen by *any* node is present.
//!
//! A departure is a delete and there is no `left_at`: a row exists only while
//! somebody is there, which is what makes `present` a single predicate rather
//! than a join against a history. The sweep that removes rows for a crashed
//! node is scheduled, which is why this shares `SchedulingConfig` with `job`.

use jails_contracts::RenderedMigration;
use jails_model::{AppModel, Component, ComponentKind, StableId};
use std::collections::BTreeSet;

fn table(component: &Component) -> String {
    format!("{}_presence", component.label)
}

/// The presence table, for a component the accepted model does not have.
pub(super) fn migrations(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    next.components
        .values()
        .filter(|component| component.kind == ComponentKind::Presence)
        .filter(|component| {
            accepted.is_none_or(|accepted| !accepted.components.contains_key(&component.id))
        })
        .map(|component| {
            let table = table(component);
            RenderedMigration {
                logical_name: format!("create_{table}"),
                bytes: migration(&table).into_bytes(),
                semantic_ids: BTreeSet::from([component.id.as_str().to_string()]),
            }
        })
        .collect()
}

/// The table, and the one index both of its queries are.
fn migration(table: &str) -> String {
    format!(
        "-- Presence, shared: one row per (scope, member, node) while that node\n\
         -- believes the member is connected. A member seen by any node is\n\
         -- present, which is the answer a single process's memory cannot give.\n\
         create table {table} (\n\
        \x20 scope text not null check (length(trim(scope)) > 0),\n\
        \x20 member text not null check (length(trim(member)) > 0),\n\
        \x20 node text not null check (length(trim(node)) > 0),\n\
        \x20 seen_at timestamptz not null,\n\
        \x20 primary key (scope, member, node)\n\
         );\n\n\
         -- The sweep deletes by age across every scope, and `present` reads one\n\
         -- scope by age. Both are this index.\n\
         create index {table}_seen_at_idx on {table} (seen_at);\n"
    )
}
