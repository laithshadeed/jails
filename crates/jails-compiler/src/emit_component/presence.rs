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

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::RenderedMigration;
use jails_model::{AppModel, Component, ComponentKind, StableId};
use std::collections::BTreeSet;

const PORT: crate::Template = crate::template!("spring/presence_port_java.java");
const STORE: crate::Template = crate::template!("spring/presence_store_java.java");
const IT: crate::Template = crate::template!("spring/presence_it_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    if !super::has_database(model) {
        return Err(CompileError::new(format!(
            "component presence `{}` needs PostgreSQL/JDBC: presence held in one process's memory is correct on one node and wrong on two, with nothing to say which\n       fix: declare `storage postgres` in the model, or run `jails add db`",
            component.name
        )));
    }
    let name = &component.name;
    let app = package(model, Package::Application);
    let adapters = package(model, Package::AdaptersJdbc);
    let table = table(component);
    let port = format!("{name}Presence");
    let import = |user: &str, owner: &str, class: &str| {
        if user == owner {
            String::new()
        } else {
            format!("import {owner}.{class};\n")
        }
    };
    // The container config is a fact about the *model* here, not a file on
    // disk -- the legacy generator reads the test tree for it, which is the
    // "source as a database" pattern this path exists to remove. It is a
    // different question from whether SQL is reachable: the guard above
    // passes for a project carrying its own JDBC starter, and that project
    // has no `TestcontainersConfig` for this test to import.
    let support = super::container_support(model, &adapters);
    Ok(vec![
        java(
            component,
            "port",
            &app,
            &port,
            false,
            // The port is managed ABI: the store and every caller name it.
            false,
            PORT.resolve(templates)?
                .replace("{{app}}", &app)
                .replace("{{name}}", name),
        )?,
        java(
            component,
            "store",
            &adapters,
            &format!("Jdbc{port}"),
            false,
            true,
            STORE
                .resolve(templates)?
                .replace("{{adapters}}", &adapters)
                .replace("{{name}}", name)
                .replace("{{table}}", &table)
                .replace("{{port_import}}", &import(&adapters, &app, &port))
                .replace("{{property}}", &component.label.replace('_', "-")),
        )?,
        java(
            component,
            "it",
            &adapters,
            &format!("Jdbc{port}IT"),
            true,
            true,
            IT.resolve(templates)?
                .replace("{{adapters}}", &adapters)
                .replace("{{name}}", name)
                .replace("{{table}}", &table)
                .replace("{{container_import}}", &support.import)
                .replace("{{container_annotation}}", support.annotation)
                .replace("{{disabled_import}}", support.disabled_import)
                .replace("{{annotation}}", support.disabled),
        )?,
    ])
}

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
