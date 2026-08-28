//! Stable-ID schema diffing and conservative PostgreSQL migration lowering.

mod index;
mod sqlite;

use crate::CompileError;
use jails_contracts::{RenderedMigration, WorkspaceSnapshot};
use jails_model::{
    AppModel, BuiltinType, ColumnRenamePolicy, Entity, Facet, Field, FieldAddPolicy,
    FieldEvolutionPolicy, Index, IndexDirection, ModelPatch, StableId, StorageRetirementPolicy,
    TypeChangeStrategy, TypeRef,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn derive(
    snapshot: &WorkspaceSnapshot,
    next: &AppModel,
    patch: Option<&ModelPatch>,
) -> Result<Vec<RenderedMigration>, CompileError> {
    let next_database = has_database(next);
    let accepted = snapshot.accepted_model.as_ref();
    let previous_database = accepted.is_some_and(has_database);
    let mut rendered = sqlite::derive(accepted, next);
    if !next_database && !previous_database && rendered.is_empty() {
        return Ok(Vec::new());
    }
    if accepted.is_none() && !snapshot.migration_history.records.is_empty() {
        return Err(CompileError::new(
            "existing migrations have no canonical accepted-schema lock\n       fix: import the existing schema before enabling the canonical `db` capability",
        ));
    }
    if !next_database && !previous_database {
        return Ok(rendered);
    }
    if previous_database && !next_database {
        return Err(CompileError::new(
            "removing canonical `db` would abandon accepted storage\n       fix: retire every table through an explicit schema policy before removing `db`",
        ));
    }

    let empty = AppModel {
        schema: next.schema.clone(),
        project: next.project.clone(),
        capabilities: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        settings: BTreeMap::new(),
        ejections: BTreeMap::new(),
        units: BTreeMap::new(),
        components: BTreeMap::new(),
        entities: BTreeMap::new(),
        operations: BTreeMap::new(),
    };
    let previous = if previous_database {
        accepted.expect("previous database model was checked")
    } else {
        &empty
    };
    let policies = evolution_policies(patch);
    let mut statements = Vec::new();
    let mut semantic_ids = BTreeSet::new();
    let mut descriptions = Vec::new();

    for old in previous
        .entities
        .values()
        .filter(|entity| entity.facets.contains(&Facet::Repository))
    {
        let Some(current) = next.entities.get(&old.id) else {
            match policies.retirements.get(old.id.as_str()) {
                Some(StorageRetirementPolicy::Drop { confirmed_table })
                    if confirmed_table == &old.names.sql_table =>
                {
                    statements.push(format!("drop table {};", old.names.sql_table));
                    semantic_ids.insert(old.id.as_str().to_string());
                    descriptions.push(format!("drop_{}", old.names.sql_table));
                    continue;
                }
                Some(StorageRetirementPolicy::Drop { confirmed_table }) => {
                    return Err(CompileError::new(format!(
                        "confirmed table `{confirmed_table}` is not accepted table `{}`\n       fix: pass `--confirm-table {}` exactly",
                        old.names.sql_table, old.names.sql_table
                    )));
                }
                _ => {
                    return Err(CompileError::new(format!(
                        "accepted table `{}` was removed without a retirement policy\n       fix: use canonical schema retirement before removing `{}`",
                        old.names.sql_table, old.label
                    )));
                }
            }
        };
        if old.active && !current.active {
            let mut expected = old.clone();
            expected.active = false;
            if policies.retirements.get(old.id.as_str()) != Some(&StorageRetirementPolicy::Preserve)
                || expected != *current
            {
                return Err(CompileError::new(format!(
                    "accepted table `{}` was deactivated without a preserve-storage policy\n       fix: use `destroy scaffold {} --storage preserve`",
                    old.names.sql_table, old.names.java_type
                )));
            }
            continue;
        }
        if !old.active && current.active {
            let Some(confirmed) = policies.revivals.get(old.id.as_str()) else {
                return Err(CompileError::new(format!(
                    "preserved table `{}` was reactivated without a revive policy\n       fix: use `resource revive {} --table {}`",
                    old.names.sql_table, old.names.java_type, old.names.sql_table
                )));
            };
            let mut expected = old.clone();
            expected.active = true;
            if confirmed != &old.names.sql_table || expected != *current {
                return Err(CompileError::new(format!(
                    "revival does not match preserved table `{}`\n       fix: revive the unchanged entity with `--table {}`",
                    old.names.sql_table, old.names.sql_table
                )));
            }
            continue;
        }
        if !old.active {
            if old != current {
                return Err(CompileError::new(format!(
                    "retired entity `{}` changed while its storage is preserved\n       fix: revive it before evolving its schema",
                    old.label
                )));
            }
            continue;
        }
        if !current.facets.contains(&Facet::Repository) {
            return Err(CompileError::new(format!(
                "accepted table `{}` lost its repository facet\n       fix: retire its storage explicitly before changing facets",
                old.names.sql_table
            )));
        }
        if old.names.sql_table != current.names.sql_table {
            return Err(CompileError::new(format!(
                "table `{}` was renamed to `{}` without a migration policy\n       fix: use canonical rename with an explicit cutover policy",
                old.names.sql_table, current.names.sql_table
            )));
        }
        for old_field in old.fields.values() {
            let Some(current_field) = current.fields.get(&old_field.id) else {
                let Some(confirmed) = policies.removals.get(old_field.id.as_str()) else {
                    return Err(CompileError::new(format!(
                        "accepted column `{}.{}` was removed without a drop policy\n       fix: use canonical field drop with exact column confirmation",
                        old.names.sql_table, old_field.names.sql_column
                    )));
                };
                statements.extend(drop_column(old, old_field, confirmed)?);
                semantic_ids.extend([
                    old.id.as_str().to_string(),
                    old_field.id.as_str().to_string(),
                ]);
                descriptions.push(format!("drop_{}", old_field.names.sql_column));
                continue;
            };
            if old_field != current_field {
                let Some(policy) = policies.replacements.get(old_field.id.as_str()) else {
                    return Err(unsupported_change(old, old_field));
                };
                let change = evolve_field(next, old, old_field, current_field, policy)?;
                if !change.is_empty() {
                    statements.extend(change);
                    semantic_ids.extend([
                        old.id.as_str().to_string(),
                        old_field.id.as_str().to_string(),
                    ]);
                    descriptions.push(format!("evolve_{}", old_field.names.sql_column));
                }
            }
        }
        for field in current
            .fields
            .values()
            .filter(|field| !old.fields.contains_key(&field.id))
        {
            statements.extend(add_column(
                next,
                current,
                field,
                policies.additions.get(field.id.as_str()),
            )?);
            semantic_ids.extend([
                current.id.as_str().to_string(),
                field.id.as_str().to_string(),
            ]);
            descriptions.push(format!(
                "add_{}_to_{}",
                field.names.sql_column, current.names.sql_table
            ));
        }
        index::derive_changes(
            old,
            current,
            &policies.index_removals,
            &mut statements,
            &mut semantic_ids,
            &mut descriptions,
        )?;
        for index in current
            .indexes
            .values()
            .filter(|index| !old.indexes.contains_key(&index.id))
        {
            statements.push(create_index(current, index)?);
            semantic_ids.extend([
                current.id.as_str().to_string(),
                index.id.as_str().to_string(),
            ]);
            descriptions.push(format!("add_{}", index.sql_name));
        }
    }

    for entity in next.entities.values().filter(|entity| {
        entity.active
            && entity.facets.contains(&Facet::Repository)
            && !previous.entities.contains_key(&entity.id)
    }) {
        statements.extend(create_table(next, entity)?);
        semantic_ids.insert(entity.id.as_str().to_string());
        semantic_ids.extend(entity.fields.keys().map(|field| field.as_str().to_string()));
        descriptions.push(format!("create_{}", entity.names.sql_table));
    }

    if statements.is_empty() {
        return Ok(rendered);
    }
    if next.project.dialect != "postgresql" {
        return Err(CompileError::new(format!(
            "canonical schema evolution does not lower dialect `{}` yet\n       fix: use `dialect = \"postgresql\"` or add a typed dialect backend",
            next.project.dialect
        )));
    }
    let logical_name = if descriptions.len() == 1 {
        descriptions.remove(0)
    } else {
        "evolve_application_schema".to_string()
    };
    let mut bytes = b"-- Generated by jails from the accepted semantic schema.\n".to_vec();
    bytes.extend(statements.join("\n").as_bytes());
    bytes.push(b'\n');
    rendered.push(RenderedMigration {
        logical_name,
        bytes,
        semantic_ids,
    });
    Ok(rendered)
}

fn has_database(model: &AppModel) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
}

#[derive(Default)]
struct EvolutionPolicies {
    additions: BTreeMap<String, FieldAddPolicy>,
    replacements: BTreeMap<String, FieldEvolutionPolicy>,
    removals: BTreeMap<String, String>,
    index_removals: BTreeMap<String, String>,
    retirements: BTreeMap<String, StorageRetirementPolicy>,
    revivals: BTreeMap<String, String>,
}

fn evolution_policies(patch: Option<&ModelPatch>) -> EvolutionPolicies {
    fn collect(patch: &ModelPatch, output: &mut EvolutionPolicies) {
        match patch {
            ModelPatch::Batch(patches) => {
                for patch in patches {
                    collect(patch, output);
                }
            }
            ModelPatch::AddField { field, policy, .. } => {
                output
                    .additions
                    .insert(field.id.as_str().to_string(), policy.clone());
            }
            ModelPatch::ReplaceField { field, policy, .. } => {
                output
                    .replacements
                    .insert(field.as_str().to_string(), policy.clone());
            }
            ModelPatch::RemoveField {
                field,
                confirmed_column,
                ..
            } => {
                output
                    .removals
                    .insert(field.as_str().to_string(), confirmed_column.clone());
            }
            ModelPatch::RemoveIndex {
                index,
                confirmed_name,
                ..
            } => {
                output
                    .index_removals
                    .insert(index.as_str().to_string(), confirmed_name.clone());
            }
            ModelPatch::RetireEntity { entity, policy } => {
                output
                    .retirements
                    .insert(entity.as_str().to_string(), policy.clone());
            }
            ModelPatch::ReviveEntity {
                entity,
                confirmed_table,
            } => {
                output
                    .revivals
                    .insert(entity.as_str().to_string(), confirmed_table.clone());
            }
            _ => {}
        }
    }
    let mut output = EvolutionPolicies::default();
    if let Some(patch) = patch {
        collect(patch, &mut output);
    }
    output
}

fn unsupported_change(entity: &Entity, before: &Field) -> CompileError {
    CompileError::new(format!(
        "accepted column `{}.{}` changed without an evolution policy\n       fix: use the canonical rename, type, nullability, or index command for this change",
        entity.names.sql_table, before.names.sql_column
    ))
}

fn evolve_field(
    model: &AppModel,
    entity: &Entity,
    before: &Field,
    after: &Field,
    policy: &FieldEvolutionPolicy,
) -> Result<Vec<String>, CompileError> {
    match policy {
        FieldEvolutionPolicy::Rename { column } => {
            let mut expected = before.clone();
            expected.names.java_member = after.names.java_member.clone();
            match column {
                ColumnRenamePolicy::Preserve => {}
                ColumnRenamePolicy::SingleCutover => {
                    expected.names.sql_column = after.names.sql_column.clone();
                }
            }
            if expected != *after {
                return Err(unsupported_change(entity, before));
            }
            if *column == ColumnRenamePolicy::Preserve {
                return Ok(Vec::new());
            }
            if before.names.sql_column == after.names.sql_column {
                return Err(CompileError::new(format!(
                    "column `{}` did not change during single-cutover rename\n       fix: choose a Java field name with a different SQL projection, or use `--column preserve`",
                    before.names.sql_column
                )));
            }
            Ok(vec![format!(
                "alter table {} rename column {} to {};",
                entity.names.sql_table, before.names.sql_column, after.names.sql_column
            )])
        }
        FieldEvolutionPolicy::ChangeType { strategy } => {
            let mut expected = before.clone();
            expected.ty = after.ty.clone();
            if expected != *after {
                return Err(unsupported_change(entity, before));
            }
            if *strategy != TypeChangeStrategy::Safe {
                return Err(CompileError::new(
                    "only proven safe field widening lowers as one canonical migration\n       fix: use `--strategy safe`, or model expand/contract as a multi-release campaign",
                ));
            }
            let from = sql_type(model, before)?;
            let to = sql_type(model, after)?;
            if !safe_widening(from, to) {
                return Err(CompileError::new(format!(
                    "changing `{}.{}` from `{from}` to `{to}` is not a proven safe widening\n       fix: use an explicit expand/contract campaign",
                    entity.names.sql_table, before.names.sql_column
                )));
            }
            Ok(vec![format!(
                "alter table {} alter column {} type {};",
                entity.names.sql_table, before.names.sql_column, to
            )])
        }
        FieldEvolutionPolicy::SetNullability { backfill_sql } => {
            let mut expected = before.clone();
            expected.required = after.required;
            if expected != *after {
                return Err(unsupported_change(entity, before));
            }
            if before.required == after.required {
                return Err(CompileError::new(format!(
                    "column `{}.{}` already has the requested nullability\n       fix: request the opposite nullability",
                    entity.names.sql_table, before.names.sql_column
                )));
            }
            if before.primary_key && !after.required {
                return Err(CompileError::new(format!(
                    "primary-key column `{}.{}` cannot be nullable\n       fix: keep the key required, or introduce a separate nullable field",
                    entity.names.sql_table, before.names.sql_column
                )));
            }
            if after.required {
                let sql = backfill_sql.as_deref().ok_or_else(|| {
                    CompileError::new(format!(
                        "making `{}.{}` required needs an explicit backfill\n       fix: pass `--backfill-file <project-path>`",
                        entity.names.sql_table, before.names.sql_column
                    ))
                })?;
                Ok(vec![
                    reader_sql(sql)?.to_string(),
                    format!(
                        "alter table {} alter column {} set not null;",
                        entity.names.sql_table, before.names.sql_column
                    ),
                ])
            } else {
                if backfill_sql.is_some() {
                    return Err(CompileError::new(
                        "making a field nullable does not need a backfill\n       fix: remove `--backfill-file`",
                    ));
                }
                Ok(vec![format!(
                    "alter table {} alter column {} drop not null;",
                    entity.names.sql_table, before.names.sql_column
                )])
            }
        }
    }
}

fn drop_column(
    entity: &Entity,
    field: &Field,
    confirmed: &str,
) -> Result<Vec<String>, CompileError> {
    if field.primary_key {
        return Err(CompileError::new(format!(
            "primary-key column `{}.{}` cannot be dropped by field evolution\n       fix: migrate to a replacement key explicitly first",
            entity.names.sql_table, field.names.sql_column
        )));
    }
    if confirmed != field.names.sql_column {
        return Err(CompileError::new(format!(
            "column confirmation `{confirmed}` does not match `{}.{}`\n       fix: pass `--confirm-column {}` exactly",
            entity.names.sql_table, field.names.sql_column, field.names.sql_column
        )));
    }
    Ok(vec![format!(
        "alter table {} drop column {};",
        entity.names.sql_table, field.names.sql_column
    )])
}

fn safe_widening(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("integer", "bigint")
            | ("integer", "numeric")
            | ("integer", "double precision")
            | ("bigint", "numeric")
            | ("bigint", "double precision")
    )
}

fn reader_sql(bytes: &[u8]) -> Result<&str, CompileError> {
    let sql = std::str::from_utf8(bytes).map_err(|_| {
        CompileError::new(
            "reader-owned backfill is not UTF-8 SQL\n       fix: save the backfill as UTF-8 text",
        )
    })?;
    let sql = sql.trim();
    if sql.is_empty() {
        return Err(CompileError::new(
            "reader-owned backfill is empty\n       fix: provide the data update that makes the constraint safe",
        ));
    }
    Ok(sql)
}

fn create_table(model: &AppModel, entity: &Entity) -> Result<Vec<String>, CompileError> {
    let mut columns = Vec::new();
    let mut indexes = Vec::new();
    for field in entity.fields.values() {
        let mut column = format!("    {} {}", field.names.sql_column, sql_type(model, field)?);
        if field.required {
            column.push_str(" not null");
        }
        if field.primary_key {
            column.push_str(" primary key");
        } else if field.unique {
            column.push_str(" unique");
        }
        if field.non_blank {
            column.push_str(&format!(
                " check (length(btrim({})) > 0)",
                field.names.sql_column
            ));
        }
        if let Some(check) = length_check(field) {
            column.push_str(&format!(" check ({check})"));
        }
        columns.push(column);
        if field.indexed && !field.primary_key && !field.unique {
            indexes.push(format!(
                "create index idx_{}_{} on {} ({});",
                entity.names.sql_table,
                field.names.sql_column,
                entity.names.sql_table,
                field.names.sql_column
            ));
        }
    }
    let mut output = vec![format!(
        "create table {} (\n{}\n);",
        entity.names.sql_table,
        columns.join(",\n")
    )];
    output.extend(indexes);
    for index in entity.indexes.values() {
        output.push(create_index(entity, index)?);
    }
    Ok(output)
}

fn create_index(entity: &Entity, index: &Index) -> Result<String, CompileError> {
    if index.columns.is_empty() {
        return Err(CompileError::new(format!(
            "index `{}` has no fields\n       fix: declare at least one indexed field",
            index.sql_name
        )));
    }
    let columns = index
        .columns
        .iter()
        .map(|column| {
            let field = entity.fields.get(&column.field).ok_or_else(|| {
                CompileError::new(format!(
                    "index `{}` references missing field `{}`\n       fix: repair the linked model before compiling",
                    index.sql_name, column.field
                ))
            })?;
            Ok(match column.direction {
                IndexDirection::Asc => field.names.sql_column.clone(),
                IndexDirection::Desc => format!("{} desc", field.names.sql_column),
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    Ok(format!(
        "create index {} on {} ({});",
        index.sql_name,
        entity.names.sql_table,
        columns.join(", ")
    ))
}

fn add_column(
    model: &AppModel,
    entity: &Entity,
    field: &Field,
    policy: Option<&FieldAddPolicy>,
) -> Result<Vec<String>, CompileError> {
    if field.primary_key {
        return Err(CompileError::new(format!(
            "cannot add `{}` as a second or replacement primary key\n       fix: model identity changes as an explicit evolution program",
            field.names.sql_column
        )));
    }
    let table = &entity.names.sql_table;
    let column = &field.names.sql_column;
    let ty = sql_type(model, field)?;
    let mut output = vec![format!("alter table {table} add column {column} {ty};")];
    if field.required {
        let backfill = match policy {
            Some(FieldAddPolicy::BackfillLiteral(value)) => {
                let literal = sql_literal(field, value)?;
                format!("update {table} set {column} = {literal} where {column} is null;")
            }
            Some(FieldAddPolicy::ReaderOwnedSql(bytes)) => reader_sql(bytes)?.to_string(),
            _ => {
                return Err(CompileError::new(format!(
                    "required field `{}.{}` needs a backfill for existing rows\n       fix: use `--default-literal <typed-value>` or `--backfill-file <project-path>`",
                    entity.label, field.label
                )));
            }
        };
        output.extend([
            backfill,
            format!("alter table {table} alter column {column} set not null;"),
        ]);
    } else if matches!(
        policy,
        Some(FieldAddPolicy::BackfillLiteral(_) | FieldAddPolicy::ReaderOwnedSql(_))
    ) {
        return Err(CompileError::new(format!(
            "nullable field `{}.{}` does not need a mandatory backfill\n       fix: remove `--default-literal` or make the field required",
            entity.label, field.label
        )));
    }
    if field.non_blank {
        output.push(format!(
            "alter table {table} add constraint chk_{table}_{column}_non_blank check (length(btrim({column})) > 0);"
        ));
    }
    if let Some(check) = length_check(field) {
        output.push(format!(
            "alter table {table} add constraint chk_{table}_{column}_length check ({check});"
        ));
    }
    if field.unique {
        output.push(format!(
            "alter table {table} add constraint uq_{table}_{column} unique ({column});"
        ));
    } else if field.indexed {
        output.push(format!(
            "create index idx_{table}_{column} on {table} ({column});"
        ));
    }
    Ok(output)
}

fn length_check(field: &Field) -> Option<String> {
    let range = field.length.as_ref()?;
    let column = &field.names.sql_column;
    Some(match (range.min, range.max) {
        (Some(min), Some(max)) => {
            format!("char_length({column}) between {min} and {max}")
        }
        (Some(min), None) => format!("char_length({column}) >= {min}"),
        (None, Some(max)) => format!("char_length({column}) <= {max}"),
        (None, None) => unreachable!("linked length ranges have at least one bound"),
    })
}

fn sql_type(model: &AppModel, field: &Field) -> Result<&'static str, CompileError> {
    if model.project.dialect != "postgresql" {
        return Err(CompileError::new(format!(
            "no SQL type backend for dialect `{}`",
            model.project.dialect
        )));
    }
    match &field.ty {
        TypeRef::Builtin(builtin) => Ok(match builtin {
            BuiltinType::String
            | BuiltinType::Uri
            | BuiltinType::Path
            | BuiltinType::ZoneId
            | BuiltinType::Currency => "text",
            BuiltinType::Integer => "integer",
            BuiltinType::Long => "bigint",
            BuiltinType::Double => "double precision",
            BuiltinType::Decimal => "numeric",
            BuiltinType::Boolean => "boolean",
            BuiltinType::Uuid => "uuid",
            BuiltinType::Date => "date",
            BuiltinType::DateTime => "timestamp",
            BuiltinType::Instant => "timestamptz",
            BuiltinType::Duration => "interval",
            BuiltinType::Bytes => "bytea",
        }),
        TypeRef::External(name) => Err(CompileError::new(format!(
            "project type `{name}` has no declared SQL representation\n       fix: declare a codec before storing this field"
        ))),
    }
}

fn sql_literal(field: &Field, value: &str) -> Result<String, CompileError> {
    let invalid = || {
        CompileError::new(format!(
            "`{value}` is not a valid {} backfill literal for `{}`",
            field.ty.canonical_name(),
            field.names.sql_column
        ))
    };
    match &field.ty {
        TypeRef::Builtin(BuiltinType::Integer | BuiltinType::Long) => value
            .parse::<i64>()
            .map(|number| number.to_string())
            .map_err(|_| invalid()),
        TypeRef::Builtin(BuiltinType::Double | BuiltinType::Decimal) => {
            let number = value.parse::<f64>().map_err(|_| invalid())?;
            if number.is_finite() {
                Ok(value.to_string())
            } else {
                Err(invalid())
            }
        }
        TypeRef::Builtin(BuiltinType::Boolean) => match value {
            "true" | "false" => Ok(value.to_string()),
            _ => Err(invalid()),
        },
        TypeRef::Builtin(BuiltinType::Bytes) | TypeRef::External(_) => Err(invalid()),
        TypeRef::Builtin(_) => Ok(format!("'{}'", value.replace('\'', "''"))),
    }
}
