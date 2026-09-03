//! Field-level schema evolution: the policies a patch carries, and the DDL
//! each one authorises.
//!
//! [`super`] answers *what schema does this model have* -- create table,
//! create index, the type mapping -- and this
//! answers *what is a reader allowed to change about an accepted one, and what
//! did they say they meant*. Every function here is reachable only from an
//! [`Evolution`] step that states a policy, which is the line between them:
//! nothing in this file runs for a model that is merely being compiled.

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

pub(super) fn evolution_policies(evolution: &Evolution) -> EvolutionPolicies {
    let mut output = EvolutionPolicies::default();
    for step in &evolution.steps {
        match step {
            EvolutionStep::AddField { field, policy } => {
                output
                    .additions
                    .insert(field.as_str().to_string(), policy.clone());
            }
            EvolutionStep::ReplaceField { field, policy } => {
                output
                    .replacements
                    .insert(field.as_str().to_string(), policy.clone());
            }
            EvolutionStep::RemoveField {
                field,
                confirmed_column,
            } => {
                output
                    .removals
                    .insert(field.as_str().to_string(), confirmed_column.clone());
            }
            EvolutionStep::RemoveIndex {
                index,
                confirmed_name,
            } => {
                output
                    .index_removals
                    .insert(index.as_str().to_string(), confirmed_name.clone());
            }
            EvolutionStep::RemoveRelation {
                relation,
                confirmed_name,
            } => {
                output
                    .relation_removals
                    .insert(relation.as_str().to_string(), confirmed_name.clone());
            }
            EvolutionStep::RetireEntity { entity, policy } => {
                output
                    .retirements
                    .insert(entity.as_str().to_string(), policy.clone());
            }
            EvolutionStep::RenameTable { entity, table } => {
                output
                    .table_renames
                    .insert(entity.as_str().to_string(), table.clone());
            }
            EvolutionStep::ReviveEntity {
                entity,
                confirmed_table,
            } => {
                output
                    .revivals
                    .insert(entity.as_str().to_string(), confirmed_table.clone());
            }
        }
    }
    output
}

pub(super) fn unsupported_change(entity: &Entity, before: &Field) -> Diagnostic {
    Diagnostic::new(
        "compile-column-changed-without-policy",
        format!("$.entities.{}.fields.{}", entity.label, before.label),
        format!(
            "accepted column `{}.{}` changed without an evolution policy",
            entity.names.sql_table, before.names.sql_column
        ),
        "use the canonical rename, type, nullability, or index command for this change",
    )
}

/// The migration filename's descriptive half, for one field change.
///
/// One word per policy, so a reader scanning `db/migration` sees what each
/// file did rather than that something did.
pub(super) fn describe(before: &Field, after: &Field, policy: &FieldEvolutionPolicy) -> String {
    let column = &before.names.sql_column;
    match policy {
        FieldEvolutionPolicy::Rename { .. } => {
            format!("rename_{column}_to_{}", after.names.sql_column)
        }
        FieldEvolutionPolicy::ChangeType { .. } => format!("retype_{column}"),
        FieldEvolutionPolicy::SetNullability { .. } => match after.required {
            true => format!("make_{column}_required"),
            false => format!("make_{column}_nullable"),
        },
    }
}

pub(super) fn evolve_field(
    model: &AppModel,
    entity: &Entity,
    before: &Field,
    after: &Field,
    policy: &FieldEvolutionPolicy,
) -> Result<Vec<String>, Diagnostic> {
    match policy {
        FieldEvolutionPolicy::Rename { column } => {
            let mut expected = before.clone();
            expected.names.java_member = after.names.java_member.clone();
            // **The label follows the declaration; the stable id is identity.**
            // JDL v1 derives a field's label from the name it is declared
            // under and carries `@id(...)` for identity, so a rename moves the
            // label by construction and there is no attribute that would pin
            // it. What must not move is everything else -- the type, the
            // optionality, the semantics, and under `preserve` the column,
            // which the editor pins with `@map`. Comparing the label as well
            // would make every v1 preserve-rename refuse as an unexplained
            // change.
            expected.label = after.label.clone();
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
                return Err(Diagnostic::new(
                    "compile-cutover-column-unchanged",
                    format!("$.entities.{}.fields.{}", entity.label, before.label),
                    format!(
                        "column `{}` did not change during single-cutover rename",
                        before.names.sql_column
                    ),
                    "choose a Java field name with a different SQL projection, or use `--column preserve`",
                ));
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
                return Err(Diagnostic::new(
                    "compile-widening-strategy-required",
                    format!("$.entities.{}.fields.{}", entity.label, before.label),
                    "only proven safe field widening lowers as one canonical migration",
                    "use `--strategy safe`, or model expand/contract as a multi-release campaign",
                ));
            }
            let from = sql_type(model, entity, before)?;
            let to = sql_type(model, entity, after)?;
            if !safe_widening(from, to) {
                return Err(Diagnostic::new(
                    "compile-widening-not-proven",
                    format!("$.entities.{}.fields.{}", entity.label, before.label),
                    format!(
                        "changing `{}.{}` from `{from}` to `{to}` is not a proven safe widening",
                        entity.names.sql_table, before.names.sql_column
                    ),
                    "use an explicit expand/contract campaign",
                ));
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
                return Err(Diagnostic::new(
                    "compile-nullability-unchanged",
                    format!("$.entities.{}.fields.{}", entity.label, before.label),
                    format!(
                        "column `{}.{}` already has the requested nullability",
                        entity.names.sql_table, before.names.sql_column
                    ),
                    "request the opposite nullability",
                ));
            }
            if before.primary_key && !after.required {
                return Err(Diagnostic::new(
                    "compile-primary-key-nullable",
                    format!("$.entities.{}.fields.{}", entity.label, before.label),
                    format!(
                        "primary-key column `{}.{}` cannot be nullable",
                        entity.names.sql_table, before.names.sql_column
                    ),
                    "keep the key required, or introduce a separate nullable field",
                ));
            }
            if after.required {
                let sql = backfill_sql.as_deref().ok_or_else(|| {
                    Diagnostic::new(
                        "compile-required-nullability-needs-backfill",
                        format!("$.entities.{}.fields.{}", entity.label, before.label),
                        format!(
                            "making `{}.{}` required needs an explicit backfill",
                            entity.names.sql_table, before.names.sql_column
                        ),
                        "pass `--backfill-file <project-path>`",
                    )
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
                    return Err(Diagnostic::new(
                        "compile-nullable-needs-no-backfill",
                        format!("$.entities.{}.fields.{}", entity.label, before.label),
                        "making a field nullable does not need a backfill",
                        "remove `--backfill-file`",
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
) -> Result<Vec<String>, Diagnostic> {
    if field.primary_key {
        return Err(Diagnostic::new(
            "compile-primary-key-dropped",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!(
                "primary-key column `{}.{}` cannot be dropped by field evolution",
                entity.names.sql_table, field.names.sql_column
            ),
            "migrate to a replacement key explicitly first",
        ));
    }
    if confirmed != field.names.sql_column {
        return Err(Diagnostic::new(
            "compile-column-confirmation-mismatch",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!(
                "column confirmation `{confirmed}` does not match `{}.{}`",
                entity.names.sql_table, field.names.sql_column
            ),
            format!("pass `--confirm-column {}` exactly", field.names.sql_column),
        ));
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

pub(super) fn reader_sql(bytes: &[u8]) -> Result<&str, Diagnostic> {
    let sql = std::str::from_utf8(bytes).map_err(|_| {
        Diagnostic::new(
            "compile-backfill-not-utf8",
            "$.evolution",
            "reader-owned backfill is not UTF-8 SQL",
            "save the backfill as UTF-8 text",
        )
    })?;
    let sql = sql.trim();
    if sql.is_empty() {
        return Err(Diagnostic::new(
            "compile-backfill-empty",
            "$.evolution",
            "reader-owned backfill is empty",
            "provide the data update that makes the constraint safe",
        ));
    }
    Ok(sql)
}
