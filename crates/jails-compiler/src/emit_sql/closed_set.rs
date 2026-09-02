//! The check constraint a closed set puts on every column that stores it.
//!
//! **An enum column is `text` until something constrains it.** The Java type
//! is a closed set and the bare column is not, so any string a hand-written
//! statement or a psql session writes is accepted, and the row comes back out
//! as a `valueOf` that throws. The constraint is what makes the two agree.
//!
//! Widening is a forward migration per storing table; narrowing is refused,
//! because a row may still hold the constant being dropped and jails cannot
//! ask the database from here -- so `add constraint` would fail at `flyway
//! migrate`, on whichever machine ran it first, about a command that had
//! reported success.

use crate::CompileError;
use jails_model::{AppModel, Entity, Facet, StableId as _, TypeRef};
use std::collections::BTreeSet;

/// `constraint <table>_<column>_allowed check (<column> in (...))`, when this
/// field stores a declared closed set.
pub(super) fn column_constraint(
    model: &AppModel,
    entity: &Entity,
    field: &jails_model::Field,
) -> Option<String> {
    let values = allowed(model, field)?;
    Some(format!(
        "constraint {}_{}_allowed check ({} in ({values}))",
        entity.names.sql_table, field.names.sql_column, field.names.sql_column
    ))
}

/// The quoted wire values this field's closed set allows, in declared order.
fn allowed(model: &AppModel, field: &jails_model::Field) -> Option<String> {
    let TypeRef::External(name) = &field.ty else {
        return None;
    };
    let declared = model
        .entities
        .values()
        .find(|entity| &entity.names.java_type == name && entity.facets.contains(&Facet::Enum))?;
    if declared.enum_constants.is_empty() {
        return None;
    }
    Some(
        declared
            .enum_constants
            .iter()
            .map(|constant| {
                format!(
                    "'{}'",
                    constant.wire_name.as_ref().unwrap_or(&constant.java_name)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// Every table whose constraint has to move because a closed set widened.
pub(super) fn derive_into(
    next: &AppModel,
    previous: &AppModel,
    statements: &mut Vec<String>,
    semantic_ids: &mut BTreeSet<String>,
    descriptions: &mut Vec<String>,
) -> Result<(), CompileError> {
    for declared in next
        .entities
        .values()
        .filter(|entity| entity.active && entity.facets.contains(&Facet::Enum))
    {
        let Some(accepted) = previous.entities.get(&declared.id) else {
            continue;
        };
        if accepted.enum_constants == declared.enum_constants {
            continue;
        }
        let removed = accepted
            .enum_constants
            .iter()
            .filter(|constant| {
                !declared
                    .enum_constants
                    .iter()
                    .any(|kept| kept.java_name == constant.java_name)
            })
            .map(|constant| constant.java_name.as_str())
            .collect::<Vec<_>>();
        if !removed.is_empty() {
            return Err(CompileError::new(narrowing_refusal(
                &declared.names.java_type,
                &accepted.enum_constants,
                &removed,
            )));
        }
        // **Only tables that exist.** A plain `g record` naming the same enum
        // has no `create table`, and `alter table drafts` would be
        // unappliable everywhere and reported nowhere.
        for stored in next
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            for field in &stored.fields {
                let TypeRef::External(name) = &field.ty else {
                    continue;
                };
                if name != &declared.names.java_type {
                    continue;
                }
                let Some(values) = allowed(next, field) else {
                    continue;
                };
                let (table, column) = (&stored.names.sql_table, &field.names.sql_column);
                statements.push(format!(
                    "alter table {table}\n  drop constraint if exists {table}_{column}_allowed;"
                ));
                statements.push(format!(
                    "alter table {table}\n  add constraint {table}_{column}_allowed\n  check ({column} in ({values}));"
                ));
                semantic_ids.insert(declared.id.as_str().to_string());
                semantic_ids.insert(field.id.as_str().to_string());
                descriptions.push(format!(
                    "allow_{table}_{column}_{}",
                    declared.enum_constants.len()
                ));
            }
        }
    }
    Ok(())
}

/// Why a closed set cannot lose a constant, in the words both callers use.
///
/// **One owner, because the frontend refuses first.** `g enum Status OPEN`
/// over `OPEN CLOSED` never reaches the compiler -- the JDL editor sees the
/// request is not an extension and stops -- so the sentence is written once
/// rather than twice, where it would drift.
pub fn narrowing_refusal(
    java_type: &str,
    accepted: &[jails_model::EnumConstant],
    removed: &[&str],
) -> String {
    format!(
        "`{java_type}` currently allows {}, and this drops {}. A stored row may still hold {}, which jails cannot check from here.\n       fix: keep the constant and stop writing it, or write the migration that proves no row holds it and then re-declare the enum",
        accepted
            .iter()
            .map(|constant| constant.java_name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        removed.join(", "),
        if removed.len() == 1 { "it" } else { "one" }
    )
}
