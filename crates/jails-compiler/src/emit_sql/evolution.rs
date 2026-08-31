//! Field-level schema evolution: the policies a patch carries, and the DDL
//! each one authorises.
//!
//! Split from [`super`] by secret. The parent answers *what schema does this
//! model have* -- create table, create index, the type mapping -- and this
//! answers *what is a reader allowed to change about an accepted one, and what
//! did they say they meant*. Every function here is reachable only from a
//! `ModelPatch` that states an evolution policy, which is the line between
//! them: nothing in this file runs for a model that is merely being compiled.

use super::*;

#[derive(Default)]
pub(super) struct EvolutionPolicies {
    pub(super) additions: BTreeMap<String, FieldAddPolicy>,
    pub(super) replacements: BTreeMap<String, FieldEvolutionPolicy>,
    pub(super) removals: BTreeMap<String, String>,
    pub(super) index_removals: BTreeMap<String, String>,
    pub(super) relation_removals: BTreeMap<String, String>,
    pub(super) retirements: BTreeMap<String, StorageRetirementPolicy>,
    pub(super) revivals: BTreeMap<String, String>,
    /// The SQL name a single-cutover rename moves an entity's table to.
    ///
    /// A rename with no policy is refused, because the accepted table would
    /// simply be left behind under its old name with nothing saying so. This
    /// is the reader stating the move, and the derived migration is one
    /// `alter table ... rename to`, which keeps the rows, the indexes and the
    /// constraints -- the whole reason a cutover is one statement.
    pub(super) table_renames: BTreeMap<String, String>,
}

pub(super) fn evolution_policies(patch: Option<&ModelPatch>) -> EvolutionPolicies {
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
            ModelPatch::RemoveRelation {
                relation,
                confirmed_name,
            } => {
                output
                    .relation_removals
                    .insert(relation.as_str().to_string(), confirmed_name.clone());
            }
            ModelPatch::RetireEntity { entity, policy } => {
                output
                    .retirements
                    .insert(entity.as_str().to_string(), policy.clone());
            }
            ModelPatch::RenameEntityProjection {
                entity,
                table: Some(table),
                ..
            } => {
                output
                    .table_renames
                    .insert(entity.as_str().to_string(), table.clone());
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

pub(super) fn unsupported_change(entity: &Entity, before: &Field) -> CompileError {
    CompileError::new(format!(
        "accepted column `{}.{}` changed without an evolution policy\n       fix: use the canonical rename, type, nullability, or index command for this change",
        entity.names.sql_table, before.names.sql_column
    ))
}

pub(super) fn evolve_field(
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

pub(super) fn drop_column(
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

pub(super) fn safe_widening(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("integer", "bigint")
            | ("integer", "numeric")
            | ("integer", "double precision")
            | ("bigint", "numeric")
            | ("bigint", "double precision")
    )
}

pub(super) fn reader_sql(bytes: &[u8]) -> Result<&str, CompileError> {
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
