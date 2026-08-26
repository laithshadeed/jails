//! Deterministic reconciliation between independently sourced schema snapshots.

use jails_protocol::database::{
    MigrationRisk, PlannedSchemaOp, SchemaObject, SchemaObjectId, SchemaObjectKind, SchemaOp,
    SchemaSnapshot,
};
use jails_support::Result;
use std::collections::{BTreeMap, BTreeSet};

/// Diff exact stable identities. Similar names are deliberately not promoted
/// to renames: accepting a rename is an explicit mutation request, not an
/// observation heuristic.
pub fn diff(from: &SchemaSnapshot, to: &SchemaSnapshot) -> Result<Vec<PlannedSchemaOp>> {
    if from.catalog.dialect != to.catalog.dialect {
        return Err(
            "schema authorities use different SQL dialects.\n       fix: compare snapshots from the same declared dialect."
                .into(),
        );
    }
    let mut operations = BTreeMap::new();
    for (id, before) in &from.catalog.objects {
        match to.catalog.objects.get(id) {
            None => insert(&mut operations, drop_op(id, before)),
            Some(after) if before != after => insert(&mut operations, alter_op(id, before, after)),
            Some(_) => {}
        }
    }
    for (id, object) in &to.catalog.objects {
        if !from.catalog.objects.contains_key(id) {
            insert(&mut operations, create_op(id, object));
        }
    }
    if operations.is_empty() {
        return Ok(Vec::new());
    }
    refuse_opaque_dependencies(from, to)?;
    topological(operations)
}

fn insert(operations: &mut BTreeMap<SchemaObjectId, PlannedSchemaOp>, operation: PlannedSchemaOp) {
    let id = operation_id(&operation.operation).clone();
    operations.insert(id, operation);
}

fn create_op(id: &SchemaObjectId, object: &SchemaObject) -> PlannedSchemaOp {
    PlannedSchemaOp {
        operation: SchemaOp::Create {
            id: id.clone(),
            object: object.clone(),
        },
        dependencies: dependencies(id, object),
        risks: risks(id, object, Change::Create),
    }
}

fn alter_op(id: &SchemaObjectId, before: &SchemaObject, after: &SchemaObject) -> PlannedSchemaOp {
    PlannedSchemaOp {
        operation: SchemaOp::Alter {
            id: id.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        dependencies: dependencies(id, after),
        risks: risks(id, after, Change::Alter),
    }
}

fn drop_op(id: &SchemaObjectId, object: &SchemaObject) -> PlannedSchemaOp {
    PlannedSchemaOp {
        operation: SchemaOp::Drop {
            id: id.clone(),
            object: object.clone(),
        },
        dependencies: BTreeSet::new(),
        risks: risks(id, object, Change::Drop),
    }
}

#[derive(Clone, Copy)]
enum Change {
    Create,
    Alter,
    Drop,
}

fn risks(id: &SchemaObjectId, object: &SchemaObject, change: Change) -> BTreeSet<MigrationRisk> {
    let mut risks = BTreeSet::new();
    match change {
        Change::Create => {
            risks.insert(MigrationRisk::Additive);
        }
        Change::Alter => {
            risks.insert(MigrationRisk::DataDependent);
            risks.insert(MigrationRisk::DeploymentIncompatible);
        }
        Change::Drop => {
            risks.insert(MigrationRisk::Destructive);
            if matches!(
                id.kind,
                SchemaObjectKind::PrimaryKey
                    | SchemaObjectKind::ForeignKey
                    | SchemaObjectKind::Unique
                    | SchemaObjectKind::Check
            ) {
                risks.insert(MigrationRisk::ConstraintLoss);
            }
        }
    }
    if matches!(object, SchemaObject::Opaque { .. }) {
        risks.insert(MigrationRisk::Opaque);
    }
    risks
}

fn dependencies(id: &SchemaObjectId, object: &SchemaObject) -> BTreeSet<SchemaObjectId> {
    let mut dependencies = BTreeSet::new();
    if id.kind != SchemaObjectKind::Schema {
        dependencies.insert(SchemaObjectId {
            dialect: id.dialect,
            namespace: id.namespace.clone(),
            kind: SchemaObjectKind::Schema,
            name: id.namespace.clone(),
            parent: None,
        });
    }
    if let Some(parent) = &id.parent {
        dependencies.insert(SchemaObjectId {
            dialect: id.dialect,
            namespace: parent
                .namespace
                .clone()
                .unwrap_or_else(|| id.namespace.clone()),
            kind: SchemaObjectKind::Table,
            name: parent.name.clone(),
            parent: None,
        });
    }
    if let SchemaObject::ForeignKey {
        referenced_table, ..
    } = object
    {
        dependencies.insert(referenced_table.clone());
    }
    dependencies
}

fn refuse_opaque_dependencies(from: &SchemaSnapshot, to: &SchemaSnapshot) -> Result<()> {
    let opaque = from
        .catalog
        .objects
        .iter()
        .chain(&to.catalog.objects)
        .find(|(_, object)| matches!(object, SchemaObject::Opaque { .. }));
    if let Some((id, _)) = opaque {
        return Err(format!(
            "schema reconciliation may invalidate opaque {:?} object `{}` in `{}`.\n       fix: ignore it explicitly or replace it with a supported declaration before planning a migration.",
            id.kind,
            id.name.as_str(),
            id.namespace.as_str()
        )
        .into());
    }
    if let Some(statement) = from
        .catalog
        .opaque
        .first()
        .or_else(|| to.catalog.opaque.first())
    {
        return Err(format!(
            "schema reconciliation is blocked by opaque migration `{}`.\n       fix: make that dependency explicit or use live evidence that can observe it.",
            statement.path
        )
        .into());
    }
    Ok(())
}

fn topological(
    mut remaining: BTreeMap<SchemaObjectId, PlannedSchemaOp>,
) -> Result<Vec<PlannedSchemaOp>> {
    // A drop depends on child drops, the reverse of creation containment.
    let drop_ids = remaining
        .iter()
        .filter_map(|(id, operation)| {
            matches!(operation.operation, SchemaOp::Drop { .. }).then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for child in &drop_ids {
        if let Some(parent) = parent_id(child)
            && let Some(operation) = remaining.get_mut(&parent)
        {
            operation.dependencies.insert(child.clone());
        }
    }

    let operation_ids = remaining.keys().cloned().collect::<BTreeSet<_>>();
    for operation in remaining.values_mut() {
        operation
            .dependencies
            .retain(|dependency| operation_ids.contains(dependency));
    }
    let mut pending = remaining
        .iter()
        .map(|(id, operation)| (id.clone(), operation.dependencies.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let ready = pending
            .iter()
            .find(|(_, dependencies)| dependencies.is_empty())
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                let cycle = remaining
                    .keys()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                format!(
                    "schema operation dependency cycle: {cycle}.\n       fix: split the change or make the dependency direction explicit."
                )
            })?;
        let operation = remaining.remove(&ready).expect("ready operation exists");
        pending.remove(&ready);
        for waiting in pending.values_mut() {
            waiting.remove(&ready);
        }
        ordered.push(operation);
    }
    Ok(ordered)
}

fn parent_id(id: &SchemaObjectId) -> Option<SchemaObjectId> {
    if let Some(parent) = &id.parent {
        return Some(SchemaObjectId {
            dialect: id.dialect,
            namespace: parent
                .namespace
                .clone()
                .unwrap_or_else(|| id.namespace.clone()),
            kind: SchemaObjectKind::Table,
            name: parent.name.clone(),
            parent: None,
        });
    }
    (id.kind != SchemaObjectKind::Schema).then(|| SchemaObjectId {
        dialect: id.dialect,
        namespace: id.namespace.clone(),
        kind: SchemaObjectKind::Schema,
        name: id.namespace.clone(),
        parent: None,
    })
}

fn operation_id(operation: &SchemaOp) -> &SchemaObjectId {
    match operation {
        SchemaOp::Create { id, .. } | SchemaOp::Alter { id, .. } | SchemaOp::Drop { id, .. } => id,
        SchemaOp::Rename { after, .. } => after,
    }
}

pub fn display_id(id: &SchemaObjectId) -> String {
    let parent = id
        .parent
        .as_ref()
        .map(|parent| format!("{}.", parent.name.as_str()))
        .unwrap_or_default();
    format!(
        "{}.{parent}{} ({:?})",
        id.namespace.as_str(),
        id.name.as_str(),
        id.kind
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::database::{CatalogSnapshot, SchemaProvenance, SqlDialect, SqlTypeName};
    use jails_protocol::identity::SqlName;

    fn id(kind: SchemaObjectKind, name: &str, parent: Option<&str>) -> SchemaObjectId {
        SchemaObjectId {
            dialect: SqlDialect::PostgreSql,
            namespace: SqlName::parse("public").unwrap(),
            kind,
            name: SqlName::parse(name).unwrap(),
            parent: parent.map(|table| jails_protocol::database::QualifiedSqlName {
                namespace: Some(SqlName::parse("public").unwrap()),
                name: SqlName::parse(table).unwrap(),
            }),
        }
    }

    fn snapshot(objects: BTreeMap<SchemaObjectId, SchemaObject>) -> SchemaSnapshot {
        SchemaSnapshot {
            catalog: CatalogSnapshot::new(SqlDialect::PostgreSql, objects, Vec::new()).unwrap(),
            provenance: SchemaProvenance::Declared,
            ignored_schemas: BTreeSet::new(),
            ignores_extension_owned_objects: true,
        }
    }

    #[test]
    fn creation_is_parent_first_and_drops_are_child_first() {
        let table = id(SchemaObjectKind::Table, "orders", None);
        let column = id(SchemaObjectKind::Column, "id", Some("orders"));
        let target = snapshot(BTreeMap::from([
            (table.clone(), SchemaObject::Table),
            (
                column.clone(),
                SchemaObject::Column {
                    sql_type: SqlTypeName::parse("uuid").unwrap(),
                    nullable: false,
                    ordinal: 1,
                    default_expression: None,
                    generated: None,
                    identity: None,
                    comment: None,
                },
            ),
        ]));
        let empty = snapshot(BTreeMap::new());
        let creates = diff(&empty, &target).unwrap();
        assert_eq!(operation_id(&creates[0].operation), &table);
        assert_eq!(operation_id(&creates[1].operation), &column);
        assert!(creates[1].dependencies.contains(&table));
        let drops = diff(&target, &empty).unwrap();
        assert_eq!(operation_id(&drops[0].operation), &column);
        assert_eq!(operation_id(&drops[1].operation), &table);
        assert!(drops[0].risks.contains(&MigrationRisk::Destructive));
    }

    #[test]
    fn opaque_objects_block_a_non_empty_plan() {
        let opaque = id(SchemaObjectKind::Routine, "vendor_hook", None);
        let source = snapshot(BTreeMap::from([(
            opaque,
            SchemaObject::Opaque {
                definition: "vendor language".into(),
            },
        )]));
        let error = diff(&source, &snapshot(BTreeMap::new())).unwrap_err();
        assert!(error.to_string().contains("opaque"));
    }
}
