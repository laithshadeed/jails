//! First-class composite primary-key and unique constraints.

use crate::id::{ConstraintId, FieldId};
use crate::linker::Linker;
use crate::model::Field;
use crate::source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EntityConstraint {
    pub id: ConstraintId,
    pub label: String,
    pub kind: ConstraintKind,
    pub sql_name: String,
    pub fields: Vec<FieldId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintKind {
    PrimaryKey,
    Unique,
}

pub(crate) fn link(
    linker: &mut Linker,
    entity_path: &str,
    sql_table: &str,
    fields: &[Field],
    field_labels: &BTreeMap<String, FieldId>,
    declarations: Vec<source::EntityConstraint>,
    stored: bool,
) -> BTreeMap<ConstraintId, EntityConstraint> {
    let mut constraints = BTreeMap::new();
    let mut shapes = BTreeMap::<(ConstraintKind, Vec<FieldId>), String>::new();
    let mut sql_names = BTreeMap::<String, String>::new();

    for (position, declaration) in declarations.into_iter().enumerate() {
        let path = format!("{entity_path}.constraints[{position}]");
        linker.register_id(&declaration.id, &format!("{path}.id"));
        let id = linker.constraint_id(&declaration.id, &format!("{path}.id"));
        let kind = match declaration.kind {
            source::ConstraintKind::PrimaryKey => ConstraintKind::PrimaryKey,
            source::ConstraintKind::Unique => ConstraintKind::Unique,
        };
        let mut resolved = Vec::new();
        let mut seen = BTreeSet::new();
        for (field_position, label) in declaration.fields.iter().enumerate() {
            let field_path = format!("{path}.fields[{field_position}]");
            let Some(field) = field_labels.get(label).cloned() else {
                linker.problem(
                    "model-constraint-field-reference",
                    field_path,
                    format!("`{label}` does not name a field on this entity"),
                    "name an entity field",
                );
                continue;
            };
            if !seen.insert(field.clone()) {
                linker.problem(
                    "model-constraint-field-duplicate",
                    field_path,
                    format!("field `{label}` appears twice in one constraint"),
                    "keep each field once",
                );
                continue;
            }
            if kind == ConstraintKind::PrimaryKey
                && fields
                    .iter()
                    .find(|candidate| candidate.id == field)
                    .is_some_and(|candidate| !candidate.required)
            {
                linker.problem(
                    "model-primary-key-required",
                    field_path,
                    format!("primary-key field `{label}` is nullable"),
                    "make every primary-key field required",
                );
            }
            resolved.push(field);
        }
        if resolved.is_empty() {
            linker.problem(
                "model-constraint-empty",
                format!("{path}.fields"),
                "a key constraint needs at least one field",
                "name one or more entity fields",
            );
        }
        if let Some(first) = shapes.insert((kind, resolved.clone()), path.clone()) {
            linker.problem(
                "model-constraint-duplicate",
                &path,
                format!("this constraint duplicates the one at {first}"),
                "keep one declaration for each key tuple",
            );
        }
        let columns = resolved
            .iter()
            .filter_map(|id| fields.iter().find(|field| field.id == *id))
            .map(|field| field.names.sql_column.as_str())
            .collect::<Vec<_>>()
            .join("_");
        let prefix = match kind {
            ConstraintKind::PrimaryKey => "pk",
            ConstraintKind::Unique => "uq",
        };
        let sql_name = declaration
            .name
            .unwrap_or_else(|| format!("{prefix}_{sql_table}_{columns}"));
        linker.sql_identifier(
            &sql_name,
            &format!("{path}.name"),
            crate::linker::validate::SqlName::constraint(stored),
        );
        if let Some(first) = sql_names.insert(sql_name.clone(), path.clone()) {
            linker.problem(
                "model-sql-constraint-collision",
                &path,
                format!("SQL constraint `{sql_name}` is already declared at {first}"),
                "give each constraint a unique physical name",
            );
        }
        if let Some(id) = id {
            constraints.insert(
                id.clone(),
                EntityConstraint {
                    id,
                    label: declaration.id,
                    kind,
                    sql_name,
                    fields: resolved,
                },
            );
        }
    }
    constraints
}
