//! Linking for composite and ordered database indexes.

use crate::id::{FieldId, IndexId};
use crate::linker::Linker;
use crate::model::{Field, Index, IndexColumn, IndexDirection};
use crate::source;
use std::collections::{BTreeMap, BTreeSet};

/// The entity an index belongs to, as the four things linking one needs.
///
/// **A parameter object, because the alternative was eight arguments.** The
/// three names and the storage flag are one fact -- which entity this is --
/// and passing them separately is how a call site ends up handing the label
/// where the table belongs.
pub(crate) struct Owner<'a> {
    pub(crate) path: &'a str,
    pub(crate) label: &'a str,
    pub(crate) sql_table: &'a str,
    /// Whether the entity reaches SQL at all; see `SqlName`.
    pub(crate) stored: bool,
}

pub(crate) fn link(
    linker: &mut Linker,
    owner: Owner<'_>,
    fields: &[Field],
    field_labels: &BTreeMap<String, FieldId>,
    declarations: BTreeMap<String, source::Index>,
) -> BTreeMap<IndexId, Index> {
    let Owner {
        path: entity_path,
        label: entity_label,
        sql_table,
        stored,
    } = owner;
    let mut indexes = BTreeMap::new();
    let mut index_names = fields
        .iter()
        .filter(|field| field.indexed && !field.primary_key && !field.unique)
        .map(|field| {
            (
                format!("idx_{sql_table}_{}", field.names.sql_column),
                format!("{entity_path}.fields.{}.indexed", field.label),
            )
        })
        .collect::<BTreeMap<String, String>>();
    let mut index_shapes = BTreeMap::<Vec<(FieldId, IndexDirection)>, String>::new();
    for (index_label, index) in declarations {
        let index_path = format!("{entity_path}.indexes.{index_label}");
        linker.label(&index_label, &index_path);
        linker.register_id(&index.id, &format!("{index_path}.id"));
        let id = linker.index_id(&index.id, &format!("{index_path}.id"));
        let sql_name = index
            .name
            .unwrap_or_else(|| format!("idx_{sql_table}_{index_label}"));
        linker.sql_identifier(
            &sql_name,
            &format!("{index_path}.name"),
            crate::linker::validate::SqlName::index(stored),
        );
        if let Some(first) = index_names.insert(sql_name.clone(), index_path.clone()) {
            linker.problem(
                "model-sql-index-collision",
                &index_path,
                format!("SQL index name `{sql_name}` is already used at {first}"),
                "give each declaration a unique SQL index name",
            );
        }
        if index.columns.is_empty() {
            linker.problem(
                "model-index-empty",
                format!("{index_path}.columns"),
                "an index needs at least one field",
                "name one or more entity field labels",
            );
        }
        let mut columns = Vec::new();
        let mut seen = BTreeSet::new();
        for (position, column) in index.columns.iter().enumerate() {
            let column_path = format!("{index_path}.columns[{position}]");
            let pieces = column.split_whitespace().collect::<Vec<_>>();
            let (field_label, direction) = match pieces.as_slice() {
                [field] | [field, "asc"] => (*field, IndexDirection::Asc),
                [field, "desc"] => (*field, IndexDirection::Desc),
                _ => {
                    linker.problem(
                        "model-index-column",
                        column_path,
                        format!("`{column}` is not an index field"),
                        "use `field`, `field asc`, or `field desc`",
                    );
                    continue;
                }
            };
            let Some(field) = field_labels.get(field_label).cloned() else {
                linker.problem(
                    "model-index-field-reference",
                    column_path,
                    format!("`{field_label}` does not name a field on `{entity_label}`"),
                    "name an entity field label",
                );
                continue;
            };
            if !seen.insert(field.clone()) {
                linker.problem(
                    "model-index-field-duplicate",
                    column_path,
                    format!("field `{field_label}` appears twice in one index"),
                    "remove the duplicate field",
                );
                continue;
            }
            columns.push(IndexColumn { field, direction });
        }
        let shape = columns
            .iter()
            .map(|column| (column.field.clone(), column.direction))
            .collect::<Vec<_>>();
        if let Some(first) = index_shapes.insert(shape, index_label.clone()) {
            linker.problem(
                "model-index-duplicate",
                index_path.clone(),
                format!("index `{index_label}` duplicates `{first}`"),
                "remove one duplicate index declaration",
            );
        }
        if let Some(id) = id {
            indexes.insert(
                id.clone(),
                Index {
                    id,
                    label: index_label,
                    sql_name,
                    columns,
                },
            );
        }
    }
    indexes
}
