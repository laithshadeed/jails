use super::{
    assignment_sql_value, context_parameter, context_value, operation_file, resolve_fields,
    stored_entity,
};
use crate::CompileError;
use crate::emit_java::{domain_import, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Command, Operation, Value};
use std::collections::BTreeSet;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    command: &Command,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let target = stored_entity(model, operation, &command.on, "command")?;
    let inputs = resolve_fields(operation, target, &command.fields, "input")?;
    primary_key(target)?;
    let mut imports = BTreeSet::from([
        format!(
            "{}.application.commands.{}",
            model.project.base_package,
            with_suffix(&operation.names.java_type, "Command")
        ),
        domain_import(model, target),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let context = context_parameter(model, target, &mut imports);
    for assignment in &command.semantics.assignments {
        if inputs.iter().any(|input| input.id == assignment.field) {
            return Err(CompileError::new(format!(
                "canonical command `{}` supplies field `{}` from both input and a constant assignment\n       fix: remove the field parameter or its `set` statement",
                operation.label, assignment.field
            )));
        }
    }
    let mut params = String::new();
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for field in target.fields.values() {
        let value = if inputs.iter().any(|input| input.id == field.id) {
            let member = &field.names.java_member;
            if field.required {
                InsertValue::Parameter(format!("input.{member}()"))
            } else {
                InsertValue::Parameter(format!("input.{member}().orElse(null)"))
            }
        } else if let Some(assignment) = command
            .semantics
            .assignments
            .iter()
            .find(|assignment| assignment.field == field.id)
        {
            InsertValue::Expression(assignment_sql_value(operation, field, &assignment.value)?)
        } else if field.semantics.scope.is_some() {
            InsertValue::Parameter(context_value(field, &mut imports))
        } else if field.semantics.updated {
            InsertValue::Expression("current_timestamp".to_string())
        } else if matches!(
            field.semantics.default.as_ref().map(|default| &default.value),
            Some(Value::Function { name, arguments }) if name == "uuid7" && arguments.is_empty()
        ) {
            imports.insert(format!(
                "{}.domain.TimeOrderedUuid",
                model.project.base_package
            ));
            InsertValue::Parameter("TimeOrderedUuid.next()".to_string())
        } else if field.semantics.default.is_some() {
            InsertValue::Omitted
        } else if !field.required {
            InsertValue::Parameter("(Object) null".to_string())
        } else {
            return Err(CompileError::new(format!(
                "canonical command `{}` cannot construct required field `{}`\n       fix: carry `{}` in the command or declare a typed field default",
                operation.label, field.label, field.label
            )));
        };
        match value {
            InsertValue::Parameter(value) => {
                columns.push(field.names.sql_column.as_str());
                values.push(format!(":{}", field.names.sql_column));
                params.push_str(&format!(
                    "        statement = statement.param(\"{}\", {value});\n",
                    field.names.sql_column
                ));
            }
            InsertValue::Expression(value) => {
                columns.push(field.names.sql_column.as_str());
                values.push(value);
            }
            InsertValue::Omitted => {}
        }
    }
    let returning = target
        .fields
        .values()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let insert = if columns.is_empty() {
        format!("insert into {} default values", target.names.sql_table)
    } else {
        format!(
            "insert into {} ({}) values ({})",
            target.names.sql_table,
            columns.join(", "),
            values.join(", ")
        )
    };
    let port_type = with_suffix(&operation.names.java_type, "Command");
    let type_name = format!("Jdbc{port_type}");
    let body = format!(
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public {} execute({context}{port_type}.Input input) {{\n        JdbcClient.StatementSpec statement = jdbc.sql(\"{insert} returning {returning}\");\n{params}        return statement.query({}.class).single();\n    }}\n}}",
        target.names.java_type, target.names.java_type
    );
    operation_file(
        model,
        capability_id,
        operation,
        target,
        &type_name,
        "command",
        "capability-db-command",
        imports,
        body,
    )
}

enum InsertValue {
    Parameter(String),
    Expression(String),
    Omitted,
}
