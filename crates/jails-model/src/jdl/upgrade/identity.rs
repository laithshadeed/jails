//! What the upgrade must not change, and what it does change.
//!
//! The translation beside this is a spelling rewrite; these two functions are
//! the reason it can be trusted. `preserves_identity` is the proof obligation
//! -- run on every upgrade, before anything is written -- and `notes` is the
//! part a reviewer needs told rather than proved.

use super::refuse;
use crate::{AppModel, Diagnostics, StableId};

/// What the upgrade changes about the model, in the reader's terms.
pub(super) fn notes(before: &AppModel, after: &AppModel) -> Vec<String> {
    let mut notes = Vec::new();
    for capability in after.capabilities.values() {
        if !before.capabilities.contains_key(&capability.id) {
            notes.push(format!(
                "the `{}` capability is materialized from the storage axis, which JDL v1 reads                  as one declaration; its artifacts are in the plan",
                capability.kind
            ));
        }
    }
    for entity in after.entities.values() {
        let Some(legacy) = before.entities.get(&entity.id) else {
            continue;
        };
        let legacy_order: Vec<&str> = legacy.fields.iter().map(|f| f.label.as_str()).collect();
        let current: Vec<&str> = entity.fields.iter().map(|f| f.label.as_str()).collect();
        if legacy_order != current {
            notes.push(format!(
                "`{}` keeps its fields in declaration order ({}) where the draft sorted them by                  label ({}); this moves the record's positional constructor",
                entity.names.java_type,
                current.join(", "),
                legacy_order.join(", ")
            ));
        }
    }
    notes
}

/// Prove the upgrade kept every identity and physical name the legacy model
/// had.
///
/// **Additions are allowed and subtractions are not**, which is the exact
/// shape §22 describes. v1 states more than the pre-v1 draft could: `storage`
/// materializes a `db` capability, `use scaffold` becomes explicit projection
/// nodes, and an operation's parameters link where pre-v1 left them empty. All
/// three are the upgrade working. What must not happen is a declaration
/// arriving under a new ID or writing to a different column, because the next
/// `sync` reads that as a drop and an add -- a migration nobody asked for
/// against a table that is already there.
pub(super) fn preserves_identity(before: &AppModel, after: &AppModel) -> Result<(), Diagnostics> {
    let mut lost = Vec::new();
    for entity in before.entities.values() {
        let Some(upgraded) = after.entities.get(&entity.id) else {
            lost.push(format!(
                "entity `{}` ({})",
                entity.label,
                entity.id.as_str()
            ));
            continue;
        };
        if upgraded.names != entity.names {
            lost.push(format!(
                "entity `{}` renamed {:?} -> {:?}",
                entity.label, entity.names, upgraded.names
            ));
        }
        for field in &entity.fields {
            match upgraded.field(&field.id) {
                None => lost.push(format!(
                    "field `{}.{}` ({})",
                    entity.label,
                    field.label,
                    field.id.as_str()
                )),
                Some(upgraded) if upgraded.names != field.names => lost.push(format!(
                    "field `{}.{}` renamed {:?} -> {:?}",
                    entity.label, field.label, field.names, upgraded.names
                )),
                Some(_) => {}
            }
        }
        for index in entity.indexes.values() {
            match upgraded.indexes.get(&index.id) {
                None => lost.push(format!(
                    "index `{}.{}` ({})",
                    entity.label,
                    index.label,
                    index.id.as_str()
                )),
                Some(upgraded) if upgraded.sql_name != index.sql_name => lost.push(format!(
                    "index `{}.{}` renamed `{}` -> `{}`",
                    entity.label, index.label, index.sql_name, upgraded.sql_name
                )),
                Some(_) => {}
            }
        }
    }
    for operation in before.operations.values() {
        match after.operations.get(&operation.id) {
            None => lost.push(format!(
                "operation `{}` ({})",
                operation.label,
                operation.id.as_str()
            )),
            Some(upgraded) if upgraded.names != operation.names => lost.push(format!(
                "operation `{}` renamed {:?} -> {:?}",
                operation.label, operation.names, upgraded.names
            )),
            Some(_) => {}
        }
    }
    for capability in before.capabilities.values() {
        if !after.capabilities.contains_key(&capability.id) {
            lost.push(format!(
                "capability `{}` ({})",
                capability.label,
                capability.id.as_str()
            ));
        }
    }
    for dependency in before.dependencies.values() {
        if !after.dependencies.contains_key(&dependency.id) {
            lost.push(format!("dependency `{}`", dependency.id.as_str()));
        }
    }
    for setting in before.settings.values() {
        if !after.settings.contains_key(&setting.id) {
            lost.push(format!("property `{}`", setting.id.as_str()));
        }
    }
    for ejection in before.ejections.values() {
        if !after.ejections.contains_key(&ejection.id) {
            lost.push(format!("ejection `{}`", ejection.id.as_str()));
        }
    }
    if lost.is_empty() {
        return Ok(());
    }
    Err(refuse(
        0,
        format!(
            "the upgrade would re-identify {} declaration(s): {}",
            lost.len(),
            lost.join("; ")
        ),
        "this is an upgrade defect, not a source one. Nothing was written; report it rather \
         than editing the source, because the identities above are what the accepted model \
         is keyed on.",
    ))
}
