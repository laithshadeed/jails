//! `component idempotency <Name>`: a retained result, not just a unique row.
//!
//! **The distinction is easy to lose.** A `@unique` column already gives one
//! row per key. What it withholds is the *result*: a retry finds the row,
//! fails the insert, and gets a 409 — telling a caller that never saw the
//! first response that the work happened, while still withholding what
//! happened. So this generates a receipt record, a store port, its JDBC
//! adapter, a guard and a test, and the guard has four outcomes: run, replay,
//! refuse a reused key, or tell an in-flight retry to come back.
//!
//! Domain-blind by construction: the scope is a string the caller picks, the
//! request is bytes the caller canonicalises, and the stored result is opaque.
//!
//! **The claim is one `insert ... on conflict do nothing returning`**, because
//! select-then-insert reopens the race it exists to close.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_contracts::RenderedMigration;
use jails_model::{AppModel, Component, ComponentKind, StableId};
use std::collections::BTreeSet;

const RECORD: crate::Template = crate::template!("spring/idempotency_record_java.java");
const PORT: crate::Template = crate::template!("spring/idempotency_port_java.java");
const STORE: crate::Template = crate::template!("spring/idempotency_store_java.java");
const GUARD: crate::Template = crate::template!("spring/idempotency_guard_java.java");
const TEST: crate::Template = crate::template!("spring/idempotency_test_java.java");
const MIGRATION: crate::Template = crate::template!("spring/idempotency_migration.sql");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    // Receipts that do not outlive a restart are not receipts. Checked against
    // the model rather than the build file, for the reason `auth` is: in one
    // transition the capability this same model declares has not been spliced
    // into the pom yet.
    if !super::has_database(model) {
        return Err(CompileError::new(format!(
            "component idempotency `{}` needs PostgreSQL/JDBC to keep receipts across restarts\n       fix: declare `storage postgres` in the model, or run `jails add db`",
            component.name
        )));
    }
    let name = &component.name;
    let domain = package(model, Package::Domain);
    let app = package(model, Package::Application);
    let adapters = package(model, Package::AdaptersJdbc);
    let service = package(model, Package::Service);
    let table = table(component);
    let record = format!("{name}Receipt");
    let port = format!("{name}Receipts");
    // Every file below the record names the receipt, the port, or both.
    // `import_from` skips the ones already in the unit's own package, because
    // importing a sibling does not compile -- which is what `--package ''`
    // produces.
    let receipt_holder = |text: String| {
        let mut unit = JavaUnit::from_source(&text);
        unit.import_from(&domain, &record);
        unit.import_from(&app, &port);
        unit
    };
    Ok(vec![
        java(
            component,
            "record",
            &domain,
            &record,
            false,
            // The receipt is managed ABI: the port, the store and the guard
            // all name it.
            false,
            RECORD
                .resolve(templates)?
                .replace("{{domain}}", &domain)
                .replace("{{name}}", name),
        )?,
        java(
            component,
            "port",
            &app,
            &port,
            false,
            false,
            receipt_holder(
                PORT.resolve(templates)?
                    .replace("{{app}}", &app)
                    .replace("{{name}}", name),
            ),
        )?,
        java(
            component,
            "store",
            &adapters,
            &format!("Jdbc{port}"),
            false,
            true,
            receipt_holder(
                STORE
                    .resolve(templates)?
                    .replace("{{adapters}}", &adapters)
                    .replace("{{name}}", name)
                    .replace("{{table}}", &table),
            ),
        )?,
        java(
            component,
            "guard",
            &service,
            &format!("{name}Guard"),
            false,
            true,
            receipt_holder(
                GUARD
                    .resolve(templates)?
                    .replace("{{service}}", &service)
                    .replace("{{name}}", name),
            ),
        )?,
        java(
            component,
            "test",
            &service,
            &format!("{name}GuardTest"),
            true,
            true,
            receipt_holder(
                TEST.resolve(templates)?
                    .replace("{{service}}", &service)
                    .replace("{{name}}", name),
            ),
        )?,
    ])
}

/// The receipts table this component keeps.
fn table(component: &Component) -> String {
    format!("{}_receipts", component.label)
}

/// The `create table` for a guard that is new in this model.
///
/// **Only for one that is new.** A migration is an irreproducible operation:
/// re-emitting it on every compile would append a second `create table` the
/// next `flyway migrate` fails on. So the accepted model decides, exactly as
/// the entity schema's own migrations do.
pub(super) fn migrations(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    next.components
        .values()
        .filter(|component| component.kind == ComponentKind::Idempotency)
        .filter(|component| {
            accepted.is_none_or(|accepted| !accepted.components.contains_key(&component.id))
        })
        .map(|component| {
            let table = table(component);
            RenderedMigration {
                logical_name: format!("create_{table}"),
                bytes: MIGRATION.built_in.replace("{{table}}", &table).into_bytes(),
                semantic_ids: BTreeSet::from([component.id.as_str().to_string()]),
            }
        })
        .collect()
}
