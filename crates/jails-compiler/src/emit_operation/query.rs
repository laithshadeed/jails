//! The JDBC adapter behind a linked `query`.
//!
//! Filters, ordering and a row limit, lowered to one `select` over the
//! entity's column projection.
//!
//! **The limit is bounded by default and may not be zero.** An omitted limit
//! renders [`DEFAULT_LIMIT`], not an unbounded scan: a query with no ceiling is
//! fine on the reader's laptop and is the failure mode nobody discovers until
//! the table is large. A declared zero is refused rather than treated as "no
//! limit", because the two readings of `0` are opposite and only one of them
//! can be silent.
//!
//! Ordering is emitted from the model's `SortDirection` rather than spliced
//! from text, so a column name can never reach SQL as an ordering clause — the
//! `created_at desc` trap the legacy index validator had to split on.

use super::{QueryFilter, context_parameter, context_value, java_string, operation_file, scopes};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Entity, Field, Join, Operation, Package, SortDirection};
use std::collections::BTreeSet;

pub(super) const DEFAULT_LIMIT: u32 = 100;

/// One filter's column, qualified the way the query needs it.
fn column(qualify: &impl Fn(&str) -> String, filter: &QueryFilter<'_>) -> String {
    match &filter.alias {
        Some(alias) => format!("{alias}.{}", filter.field.names.sql_column),
        None => qualify(&filter.field.names.sql_column),
    }
}

/// Everything the `select` is shaped by, resolved together by the caller.
///
/// A parameter object rather than four more positional arguments: these are
/// read off one linked `Query` in one place and consumed together here, which
/// is the same cut every other multi-value argument in this crate takes.
pub(super) struct Shape<'a> {
    pub filters: &'a [QueryFilter<'a>],
    pub ordering: &'a [(&'a Field, SortDirection)],
    pub joins: &'a [Join],
    pub limit: u32,
}

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    shape: Shape<'_>,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let Shape {
        filters,
        ordering,
        joins,
        limit,
    } = shape;
    if limit == 0 {
        return Err(CompileError::new(format!(
            "canonical query `{}` has a zero row limit\n       fix: set a positive limit or omit it for the bounded default of {DEFAULT_LIMIT}",
            operation.label
        )));
    }
    let type_name = format!("Jdbc{}", with_suffix(&operation.names.java_type, "Query"));
    let port_type = with_suffix(&operation.names.java_type, "Query");
    let mut imports = BTreeSet::from([
        format!(
            "{}.{port_type}",
            model.project.package_for(Package::ApplicationQueries)
        ),
        domain_import(model, target),
        "java.util.ArrayList".to_string(),
        "java.util.List".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    for filter in filters {
        let _ = java_type(filter.field, &mut imports);
    }
    let context = context_parameter(model, target, &mut imports);
    let scope_fields = scopes(target);
    // Qualified only when something is joined: an unqualified `id` is
    // ambiguous across two tables, and qualifying unconditionally would
    // rewrite the SQL of every query that has no join.
    let qualify = |column: &str| {
        if joins.is_empty() {
            column.to_string()
        } else {
            format!("{}.{column}", target.names.sql_table)
        }
    };
    let columns = target
        .fields
        .iter()
        .map(|field| qualify(&field.names.sql_column))
        .collect::<Vec<_>>()
        .join(", ");
    // `join users "user" on messages.user_id = "user".id`. The alias is quoted
    // because the natural one is often a reserved word -- `join User as user`
    // renders `user`, which unquoted is PostgreSQL's own function.
    let join_clauses = joins
        .iter()
        .map(|join| {
            let remote = model.entities.get(&join.entity).ok_or_else(|| {
                CompileError::new(format!(
                    "linked query `{}` joins missing entity `{}`",
                    operation.label, join.entity
                ))
            })?;
            let on = join
                .mappings
                .iter()
                .map(|mapping| {
                    let local = target.field(&mapping.local).ok_or_else(|| {
                        CompileError::new(format!(
                            "linked query `{}` joins on missing local field `{}`",
                            operation.label, mapping.local
                        ))
                    })?;
                    let remote_field = remote.field(&mapping.remote).ok_or_else(|| {
                        CompileError::new(format!(
                            "linked query `{}` joins on missing remote field `{}`",
                            operation.label, mapping.remote
                        ))
                    })?;
                    Ok(format!(
                        "{}.{} = \"{}\".{}",
                        target.names.sql_table,
                        local.names.sql_column,
                        join.alias,
                        remote_field.names.sql_column
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?
                .join(" and ");
            Ok(format!(
                " join {} \"{}\" on {on}",
                remote.names.sql_table, join.alias
            ))
        })
        .collect::<Result<Vec<_>, CompileError>>()?
        .join("");
    let predicates = filters
        .iter()
        .filter(|filter| filter.required)
        .map(|filter| format!("{} = :{}", column(&qualify, filter), filter.label))
        .chain(scope_fields.iter().map(|field| {
            format!(
                "{} = :scope_{}",
                qualify(&field.names.sql_column),
                field.names.sql_column
            )
        }))
        .collect::<Vec<_>>();
    let predicate_seed = if predicates.is_empty() {
        "new ArrayList<String>()".to_string()
    } else {
        format!(
            "new ArrayList<String>(List.of({}))",
            predicates
                .iter()
                .map(|predicate| java_string(predicate))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let optional_predicates = filters
        .iter()
        .filter(|filter| !filter.required)
        .map(|filter| {
            format!(
                "        if (input.{}().isPresent()) {{\n            predicates.add({});\n        }}\n",
                filter.member,
                java_string(&format!("{} = :{}", column(&qualify, filter), filter.label))
            )
        })
        .collect::<String>();
    // `asc` is the SQL default and the canonical formatter omits it, so only
    // `desc` is rendered. Dropping it entirely is what made a newest-first
    // query return oldest-first.
    let ordering = if ordering.is_empty() {
        String::new()
    } else {
        format!(
            "        sql.append(\" order by {}\");\n",
            ordering
                .iter()
                .map(|(field, direction)| match direction {
                    SortDirection::Asc => qualify(&field.names.sql_column),
                    SortDirection::Desc => format!("{} desc", qualify(&field.names.sql_column)),
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let required_params = filters
        .iter()
        .filter(|filter| filter.required)
        .map(|filter| {
            format!(
                "        statement = statement.param(\"{}\", {});\n",
                filter.label,
                crate::emit_sql::bound_value(
                    model,
                    filter.field,
                    &format!("input.{}()", filter.member),
                    &mut imports
                )
            )
        })
        .collect::<String>();
    let optional_params = filters
        .iter()
        .filter(|filter| !filter.required)
        .map(|filter| {
            format!(
                "        if (input.{}().isPresent()) {{\n            statement = statement.param(\"{}\", {});\n        }}\n",
                filter.member,
                filter.label,
                crate::emit_sql::bound_value(
                    model,
                    filter.field,
                    &format!("input.{}().orElseThrow()", filter.member),
                    &mut imports
                )
            )
        })
        .collect::<String>();
    let scope_params = scope_fields
        .iter()
        .map(|field| {
            let value = context_value(field, &mut imports);
            format!(
                "        statement = statement.param(\"scope_{}\", {value});\n",
                field.names.sql_column
            )
        })
        .collect::<String>();
    // Escaped once, here, rather than by each site that splices a fragment in:
    // a quoted join alias inside a Java string literal is a syntax error, and
    // the failure is in generated code rather than in this file.
    let select = java_string(&format!(
        "select {columns} from {}{join_clauses}",
        target.names.sql_table
    ));
    let body = format!(
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public List<{}> execute({context}{port_type}.Input input) {{\n        var sql = new StringBuilder({select});\n        var predicates = {predicate_seed};\n{optional_predicates}        if (!predicates.isEmpty()) {{\n            sql.append(\" where \").append(String.join(\" and \", predicates));\n        }}\n{ordering}        sql.append(\" limit {limit}\");\n        JdbcClient.StatementSpec statement = jdbc.sql(sql.toString());\n{required_params}{optional_params}{scope_params}        return statement.query({}.class).list();\n    }}\n}}",
        target.names.java_type, target.names.java_type
    );
    operation_file(
        model,
        capability_id,
        operation,
        target,
        &type_name,
        "query",
        "capability-db-query",
        imports,
        body,
    )
}
