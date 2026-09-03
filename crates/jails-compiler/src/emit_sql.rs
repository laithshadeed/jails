//! Stable-ID schema diffing and conservative PostgreSQL migration lowering.

mod binding;
pub(crate) mod closed_set;
mod evolution;
mod field_semantics;
mod index;
mod relation;
mod search;

pub(crate) use search::{COLUMN as SEARCH_COLUMN, CONFIGURATION as SEARCH_CONFIGURATION};
mod sqlite;

use evolution::{drop_column, evolution_policies, evolve_field, reader_sql, unsupported_change};

use crate::Diagnostic;
use jails_contracts::{RenderedMigration, WorkspaceSnapshot};
use jails_model::LiteralKind;
use jails_model::{
    AppModel, ColumnRenamePolicy, Entity, Evolution, EvolutionStep, Facet, Field, FieldAddPolicy,
    FieldEvolutionPolicy, Index, IndexDirection, StableId, StorageRetirementPolicy,
    TypeChangeStrategy, TypeRef,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) use binding::{bound_value, database_assigned, optional_bound_value};
use field_semantics::{SqlDefault, initial_column, length_check, numeric_check, sql_default};
pub(crate) use index::leading_fields as leading_index_fields;

pub(crate) fn derive(
    snapshot: &WorkspaceSnapshot,
    next: &AppModel,
    evolution: &Evolution,
) -> Result<Vec<RenderedMigration>, Diagnostic> {
    let next_database = has_database(next);
    let accepted = snapshot.accepted_model.as_ref();
    let previous_database = accepted.is_some_and(has_database);
    let mut rendered = sqlite::derive(accepted, next);
    if !next_database && !previous_database && rendered.is_empty() {
        return Ok(Vec::new());
    }
    if accepted.is_none() && !snapshot.migration_history.records.is_empty() {
        return Err(Diagnostic::new(
            "compile-migrations-without-accepted-lock",
            "$.capabilities.db",
            "existing migrations have no canonical accepted-schema lock",
            "import the existing schema before enabling the canonical `db` capability",
        ));
    }
    if !next_database && !previous_database {
        return Ok(rendered);
    }
    if previous_database && !next_database {
        // **Only when there is storage to abandon.** A project that declared
        // `db` and never scaffolded anything has no table, no accepted
        // migration and nothing to retire -- refusing there would make `add db`
        // a one-way door on a project where the reader has simply changed
        // their mind, with a fix line naming a policy for tables that do not
        // exist.
        let stored = accepted
            .iter()
            .flat_map(|model| model.entities.values())
            .any(|entity| entity.facets.contains(&Facet::Repository));
        if stored || !snapshot.migration_history.records.is_empty() {
            return Err(Diagnostic::new(
                "compile-storage-abandoned",
                "$.capabilities.db",
                "removing canonical `db` would abandon accepted storage",
                "retire every table through an explicit schema policy before removing `db`",
            ));
        }
        return Ok(rendered);
    }

    let empty = AppModel {
        schema: next.schema.clone(),
        language_version: next.language_version,
        convention_version: next.convention_version,
        project: next.project.clone(),
        capabilities: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        settings: BTreeMap::new(),
        ejections: BTreeMap::new(),
        units: BTreeMap::new(),
        components: BTreeMap::new(),
        projections: BTreeMap::new(),
        relations: BTreeMap::new(),
        entities: BTreeMap::new(),
        operations: BTreeMap::new(),
        derived: BTreeMap::new(),
    };
    let previous = if previous_database {
        accepted.expect("previous database model was checked")
    } else {
        &empty
    };
    let policies = evolution_policies(evolution);
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
                    return Err(Diagnostic::new(
                        "compile-table-confirmation-mismatch",
                        format!("$.entities.{}", old.label),
                        format!(
                            "confirmed table `{confirmed_table}` is not accepted table `{}`",
                            old.names.sql_table
                        ),
                        format!("pass `--confirm-table {}` exactly", old.names.sql_table),
                    ));
                }
                _ => {
                    return Err(Diagnostic::new(
                        "compile-table-removed-without-policy",
                        format!("$.entities.{}", old.label),
                        format!(
                            "accepted table `{}` was removed without a retirement policy",
                            old.names.sql_table
                        ),
                        format!(
                            "use canonical schema retirement before removing `{}`",
                            old.label
                        ),
                    ));
                }
            }
        };
        if old.active && !current.active {
            let mut expected = old.clone();
            expected.active = false;
            if policies.retirements.get(old.id.as_str()) != Some(&StorageRetirementPolicy::Preserve)
                || expected != *current
            {
                return Err(Diagnostic::new(
                    "compile-table-deactivated-without-policy",
                    format!("$.entities.{}", old.label),
                    format!(
                        "accepted table `{}` was deactivated without a preserve-storage policy",
                        old.names.sql_table
                    ),
                    format!(
                        "use `destroy scaffold {} --storage preserve`",
                        old.names.java_type
                    ),
                ));
            }
            continue;
        }
        if !old.active && current.active {
            let Some(confirmed) = policies.revivals.get(old.id.as_str()) else {
                return Err(Diagnostic::new(
                    "compile-table-revived-without-policy",
                    format!("$.entities.{}", old.label),
                    format!(
                        "preserved table `{}` was reactivated without a revive policy",
                        old.names.sql_table
                    ),
                    format!(
                        "use `resource revive {} --table {}`",
                        old.names.java_type, old.names.sql_table
                    ),
                ));
            };
            let mut expected = old.clone();
            expected.active = true;
            if confirmed != &old.names.sql_table || expected != *current {
                return Err(Diagnostic::new(
                    "compile-revival-table-mismatch",
                    format!("$.entities.{}", old.label),
                    format!(
                        "revival does not match preserved table `{}`",
                        old.names.sql_table
                    ),
                    format!(
                        "revive the unchanged entity with `--table {}`",
                        old.names.sql_table
                    ),
                ));
            }
            continue;
        }
        if !old.active {
            if old != current {
                return Err(Diagnostic::new(
                    "compile-retired-entity-changed",
                    format!("$.entities.{}", old.label),
                    format!(
                        "retired entity `{}` changed while its storage is preserved",
                        old.label
                    ),
                    "revive it before evolving its schema",
                ));
            }
            continue;
        }
        if !current.facets.contains(&Facet::Repository) {
            return Err(Diagnostic::new(
                "compile-repository-facet-dropped",
                format!("$.entities.{}", old.label),
                format!(
                    "accepted table `{}` lost its repository facet",
                    old.names.sql_table
                ),
                "retire its storage explicitly before changing facets",
            ));
        }
        if old.names.sql_table != current.names.sql_table {
            // **The cutover is one statement, and it is derived here.** The
            // rename policy states the destination; emitting the migration
            // from the same place every other schema change comes from is
            // what keeps it in the reviewed plan rather than beside it.
            if policies.table_renames.get(old.id.as_str()) != Some(&current.names.sql_table) {
                return Err(Diagnostic::new(
                    "compile-table-renamed-without-policy",
                    format!("$.entities.{}", old.label),
                    format!(
                        "table `{}` was renamed to `{}` without a migration policy",
                        old.names.sql_table, current.names.sql_table
                    ),
                    format!(
                        "`jails rename resource {} <NewName> --strategy single-cutover`, or keep the table with `--strategy preserve-table`",
                        old.names.java_type
                    ),
                ));
            }
            // **Everything the old table's name is baked into.** PostgreSQL
            // renames the table and leaves its indexes and its primary-key
            // constraint under names that still say `tasks`, which is drift
            // nobody sees until they read the schema a year later. Every one
            // of these names is derived from the table's, so the compiler
            // knows exactly which ones moved -- an index the reader named
            // themselves is theirs and stays.
            let (before, after) = (&old.names.sql_table, &current.names.sql_table);
            statements.push(format!("alter table {before} rename to {after};"));
            statements.push(format!(
                "alter table {after} rename constraint {before}_pkey to {after}_pkey;"
            ));
            for field in old.fields.iter() {
                if field.indexed && !field.primary_key && !field.unique {
                    let column = &field.names.sql_column;
                    statements.push(format!(
                        "alter index idx_{before}_{column} rename to idx_{after}_{column};"
                    ));
                }
            }
            semantic_ids.insert(old.id.as_str().to_string());
            descriptions.push(format!(
                "rename_{}_to_{}",
                old.names.sql_table, current.names.sql_table
            ));
        }
        for old_field in old.fields.iter() {
            let Some(current_field) = current.field(&old_field.id) else {
                let Some(confirmed) = policies.removals.get(old_field.id.as_str()) else {
                    return Err(Diagnostic::new(
                        "compile-column-dropped-without-policy",
                        format!("$.entities.{}.fields.{}", old.label, old_field.label),
                        format!(
                            "accepted column `{}.{}` was removed without a drop policy",
                            old.names.sql_table, old_field.names.sql_column
                        ),
                        "use canonical field drop with exact column confirmation",
                    ));
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
                    // **Named for the change, not for the fact that one
                    // happened.** A column relaxed and then made required
                    // again produces two migrations, and `evolve_description`
                    // twice is a history nobody can read.
                    descriptions.push(evolution::describe(old_field, current_field, policy));
                }
            }
        }
        for field in current
            .fields
            .iter()
            .filter(|field| !old.has_field(&field.id))
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
        semantic_ids.extend(
            entity
                .fields
                .iter()
                .map(|field| field.id.as_str().to_string()),
        );
        descriptions.push(format!("create_{}", entity.names.sql_table));
    }

    closed_set::derive_into(
        next,
        previous,
        &mut statements,
        &mut semantic_ids,
        &mut descriptions,
    )?;
    search::derive_into(
        next,
        previous,
        &mut statements,
        &mut semantic_ids,
        &mut descriptions,
    )?;
    relation::derive_into(
        &policies.relation_removals,
        next,
        previous,
        &mut statements,
        &mut semantic_ids,
        &mut descriptions,
    )?;
    if statements.is_empty() {
        return Ok(rendered);
    }
    if next.project.dialect != "postgresql" {
        return Err(Diagnostic::new(
            "compile-dialect-not-lowered",
            "$.project.dialect",
            format!(
                "canonical schema evolution does not lower dialect `{}` yet",
                next.project.dialect
            ),
            "use `dialect = \"postgresql\"` or add a typed dialect backend",
        ));
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

pub(crate) fn has_database(model: &AppModel) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
}

fn create_table(model: &AppModel, entity: &Entity) -> Result<Vec<String>, Diagnostic> {
    let mut columns = Vec::new();
    let mut indexes = Vec::new();
    // Which columns this table has is [`storage_columns`]'s answer, and the
    // DDL is rendered from the same list rather than from a second walk --
    // `doctor` asks that function the same question.
    for (field, name) in entity.fields.iter().zip(declared_columns(entity)) {
        columns.push(initial_column(
            field,
            &name,
            sql_type(model, entity, field)?,
        )?);
        // **Named, and a table constraint rather than a column check.** The
        // widening migration drops it by name and adds the next one, so the
        // two have to agree about what it is called.
        if let Some(constraint) = closed_set::column_constraint(model, entity, field) {
            columns.push(format!("    {constraint}"));
        }
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
    // **Composite keys are table constraints, not column markers.** A
    // single-column `@pk`/`@unique` rides on its column above; a tuple has no
    // column to ride on, and it is what a tenant-scoped foreign key needs --
    // PostgreSQL requires the columns a reference names to carry a unique
    // constraint of their own, so `(workspace_id, id)` needs stating even
    // where `id` alone is already the key.
    for constraint in entity.constraints.values() {
        let names = constraint
            .fields
            .iter()
            .map(|field| {
                entity
                    .field(field)
                    .map(|field| field.names.sql_column.as_str())
                    .ok_or_else(|| {
                        crate::refuse::unlinked(
                            format!("$.entities.{}", entity.label),
                            format!(
                                "linked constraint `{}` references missing field `{field}`",
                                constraint.label
                            ),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match constraint.kind {
            jails_model::ConstraintKind::PrimaryKey => "primary key",
            jails_model::ConstraintKind::Unique => "unique",
        };
        columns.push(format!(
            "    constraint {} {kind} ({})",
            constraint.sql_name,
            names.join(", ")
        ));
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

/// The columns a stored entity's `create table` declares, in order.
fn declared_columns(entity: &Entity) -> Vec<String> {
    entity
        .fields
        .iter()
        .map(|field| field.names.sql_column.clone())
        .collect()
}

/// Every column the storage lowering emits for one stored entity.
///
/// **The one answer to "what columns does this table have".** The DDL above is
/// rendered from the same list, and `doctor` compares this model's answer with
/// the accepted model's rather than reading the columns back out of migration
/// text -- a reader of emitted SQL is a second description of a decision this
/// function already made, and the two drift silently.
///
/// The declared fields in declaration order, plus the generated search column
/// where a search projection names this entity: it is a real column of the
/// table, added by its own `alter table`, and a caller listing what the table
/// has that left it out would be wrong.
pub fn storage_columns(model: &AppModel, entity: &Entity) -> Vec<String> {
    let mut columns = declared_columns(entity);
    if search::indexes(model, entity) {
        columns.push(search::COLUMN.to_string());
    }
    columns
}

fn create_index(entity: &Entity, index: &Index) -> Result<String, Diagnostic> {
    if index.columns.is_empty() {
        return Err(Diagnostic::new(
            "compile-index-without-fields",
            format!("$.entities.{}.indexes.{}", entity.label, index.label),
            format!("index `{}` has no fields", index.sql_name),
            "declare at least one indexed field",
        ));
    }
    let columns = index
        .columns
        .iter()
        .map(|column| {
            let field = entity.field(&column.field).ok_or_else(|| {
                crate::refuse::broken_link(
                    format!("$.entities.{}.indexes.{}", entity.label, index.label),
                    format!(
                        "index `{}` references missing field `{}`",
                        index.sql_name, column.field
                    ),
                )
            })?;
            Ok(match column.direction {
                IndexDirection::Asc => field.names.sql_column.clone(),
                IndexDirection::Desc => format!("{} desc", field.names.sql_column),
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
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
) -> Result<Vec<String>, Diagnostic> {
    if field.primary_key {
        return Err(Diagnostic::new(
            "compile-second-primary-key",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!(
                "cannot add `{}` as a second or replacement primary key",
                field.names.sql_column
            ),
            "model identity changes as an explicit evolution program",
        ));
    }
    let table = &entity.names.sql_table;
    let column = &field.names.sql_column;
    let ty = sql_type(model, entity, field)?;
    let mut output = vec![format!("alter table {table} add column {column} {ty};")];
    if field.required {
        let backfill = match policy {
            Some(FieldAddPolicy::BackfillLiteral(value)) => {
                let literal = sql_literal(entity, field, value)?;
                format!("update {table} set {column} = {literal} where {column} is null;")
            }
            Some(FieldAddPolicy::ReaderOwnedSql(bytes)) => reader_sql(bytes)?.to_string(),
            _ => {
                return Err(Diagnostic::new(
                    "compile-required-field-needs-backfill",
                    format!("$.entities.{}.fields.{}", entity.label, field.label),
                    format!(
                        "required field `{}.{}` needs a backfill for existing rows",
                        entity.label, field.label
                    ),
                    "use `--default-literal <typed-value>` or `--backfill-file <project-path>`",
                ));
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
        return Err(Diagnostic::new(
            "compile-backfill-without-required-field",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!(
                "nullable field `{}.{}` does not need a mandatory backfill",
                entity.label, field.label
            ),
            "remove `--default-literal` or make the field required",
        ));
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
    if let Some(check) = numeric_check(field) {
        output.push(format!(
            "alter table {table} add constraint chk_{table}_{column}_numeric check ({check});"
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
    match sql_default(field)? {
        Some(SqlDefault::Expression(value)) => output.push(format!(
            "alter table {table} alter column {column} set default {value};"
        )),
        Some(SqlDefault::Identity) => {
            return Err(Diagnostic::new(
                "compile-identity-field-added",
                format!("$.entities.{}.fields.{}", entity.label, field.label),
                format!(
                    "identity field `{}` cannot be added as an ordinary field",
                    field.label
                ),
                "model primary-key creation with the entity",
            ));
        }
        Some(SqlDefault::Application) | None => {}
    }
    Ok(output)
}

fn sql_type(model: &AppModel, entity: &Entity, field: &Field) -> Result<&'static str, Diagnostic> {
    if model.project.dialect != "postgresql" {
        return Err(Diagnostic::without_a_fix(
            "compile-dialect-without-type-backend",
            "$.project.dialect",
            format!(
                "no SQL type backend for dialect `{}`",
                model.project.dialect
            ),
        ));
    }
    match &field.ty {
        TypeRef::Builtin(builtin) => Ok(builtin.semantics().sql_postgres),
        // **A declared enum stores as its constant's name**, which is what the
        // Spring converter reads back. It is derivable in a way an arbitrary
        // project type is
        // not: the model holds the constants, so the column's domain is known
        // rather than guessed, and no codec has to be declared for a fact the
        // model already states.
        TypeRef::External(name) if declares_enum(model, name) => Ok("text"),
        TypeRef::External(name) => Err(Diagnostic::new(
            "compile-project-type-without-sql-type",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!("project type `{name}` has no declared SQL representation"),
            "declare a codec before storing this field",
        )),
    }
}

/// Does the model declare `name` as an enum?
///
/// By Java type, because that is what a field's `External` type names.
pub(super) fn declares_enum(model: &AppModel, name: &str) -> bool {
    model.entities.values().any(|entity| {
        entity.names.java_type == name && entity.facets.contains(&jails_model::Facet::Enum)
    })
}

fn sql_literal(entity: &Entity, field: &Field, value: &str) -> Result<String, Diagnostic> {
    let invalid = || {
        Diagnostic::without_a_fix(
            "compile-backfill-literal-invalid",
            format!("$.entities.{}.fields.{}", entity.label, field.label),
            format!(
                "`{value}` is not a valid {} backfill literal for `{}`",
                field.ty.canonical_name(),
                field.names.sql_column
            ),
        )
    };
    let builtin = match &field.ty {
        TypeRef::Builtin(builtin) => *builtin,
        TypeRef::External(_) => return Err(invalid()),
    };
    // Grouped by how the builtin's literal is written, which is the row's
    // `literal` -- the same question `link_default` asks. Both parsed as
    // `i64`: a backfill literal is checked for shape here and by the column's
    // own type when the migration runs, so narrowing `int` to 32 bits twice
    // would only change which of the two reports it.
    match builtin.semantics().literal {
        LiteralKind::Int32 | LiteralKind::Int64 => value
            .parse::<i64>()
            .map(|number| number.to_string())
            .map_err(|_| invalid()),
        LiteralKind::Fractional => {
            let number = value.parse::<f64>().map_err(|_| invalid())?;
            if number.is_finite() {
                Ok(value.to_string())
            } else {
                Err(invalid())
            }
        }
        LiteralKind::Boolean => match value {
            "true" | "false" => Ok(value.to_string()),
            _ => Err(invalid()),
        },
        LiteralKind::Opaque => Err(invalid()),
        LiteralKind::Text => Ok(format!("'{}'", value.replace('\'', "''"))),
    }
}
