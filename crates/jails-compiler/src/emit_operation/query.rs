use super::{context_parameter, context_value, java_string, operation_file, scopes};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Entity, Field, Operation, SortDirection};
use std::collections::BTreeSet;

pub(super) const DEFAULT_LIMIT: u32 = 100;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    filters: &[&Field],
    ordering: &[(&Field, SortDirection)],
    limit: u32,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
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
            model.project.package_for("application.queries")
        ),
        domain_import(model, target),
        "java.util.ArrayList".to_string(),
        "java.util.List".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    for field in filters {
        let _ = java_type(field, &mut imports);
    }
    let context = context_parameter(model, target, &mut imports);
    let scope_fields = scopes(target);
    let columns = target
        .fields
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let predicates = filters
        .iter()
        .filter(|field| field.required)
        .map(|field| format!("{} = :{}", field.names.sql_column, field.label))
        .chain(scope_fields.iter().map(|field| {
            format!(
                "{} = :scope_{}",
                field.names.sql_column, field.names.sql_column
            )
        }))
        .collect::<Vec<_>>();
    let predicate_seed = if predicates.is_empty() {
        "new ArrayList<>()".to_string()
    } else {
        format!(
            "new ArrayList<>(List.of({}))",
            predicates
                .iter()
                .map(|predicate| java_string(predicate))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let optional_predicates = filters
        .iter()
        .filter(|field| !field.required)
        .map(|field| {
            format!(
                "        if (input.{}().isPresent()) {{\n            predicates.add(\"{} = :{}\");\n        }}\n",
                field.names.java_member, field.names.sql_column, field.label
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
                    SortDirection::Asc => field.names.sql_column.clone(),
                    SortDirection::Desc => format!("{} desc", field.names.sql_column),
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let required_params = filters
        .iter()
        .filter(|field| field.required)
        .map(|field| {
            format!(
                "        statement = statement.param(\"{}\", input.{}());\n",
                field.label, field.names.java_member
            )
        })
        .collect::<String>();
    let optional_params = filters
        .iter()
        .filter(|field| !field.required)
        .map(|field| {
            format!(
                "        if (input.{}().isPresent()) {{\n            statement = statement.param(\"{}\", input.{}().orElseThrow());\n        }}\n",
                field.names.java_member, field.label, field.names.java_member
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
    let body = format!(
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public List<{}> execute({context}{port_type}.Input input) {{\n        var sql = new StringBuilder(\"select {columns} from {}\");\n        var predicates = {predicate_seed};\n{optional_predicates}        if (!predicates.isEmpty()) {{\n            sql.append(\" where \").append(String.join(\" and \", predicates));\n        }}\n{ordering}        sql.append(\" limit {limit}\");\n        JdbcClient.StatementSpec statement = jdbc.sql(sql.toString());\n{required_params}{optional_params}{scope_params}        return statement.query({}.class).list();\n    }}\n}}",
        target.names.java_type, target.names.sql_table, target.names.java_type
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
