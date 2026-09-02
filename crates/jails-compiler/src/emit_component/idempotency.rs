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

use jails_contracts::RenderedMigration;
use jails_model::{AppModel, Component, ComponentKind, StableId};
use std::collections::BTreeSet;

const MIGRATION: crate::Template = crate::template!("spring/idempotency_migration.sql");

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
