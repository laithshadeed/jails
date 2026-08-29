use super::{
    assignment_sql_value, context_parameter, context_value, operation_file, resolve_fields,
    stored_entity,
};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{
    AppModel, Command, ConstraintKind, Entity, Field, Operation, OperationParameter,
    ParameterSource, TypeRef, Value,
};
use std::collections::{BTreeMap, BTreeSet};

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
            "{}.{}",
            model.project.package_for("application.commands"),
            with_suffix(&operation.names.java_type, "Command")
        ),
        domain_import(model, target),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let context = context_parameter(model, target, &mut imports);
    let rich_inputs = local_parameters(command, target)?;
    for assignment in &command.semantics.assignments {
        if inputs.iter().any(|input| input.id == assignment.field)
            || rich_inputs.contains_key(&assignment.field)
        {
            return Err(CompileError::new(format!(
                "canonical command `{}` supplies field `{}` from both input and a constant assignment\n       fix: remove the field parameter or its `set` statement",
                operation.label, assignment.field
            )));
        }
    }
    let (resolved_values, resolutions) = lower_resolutions(
        model,
        operation,
        command,
        target,
        &rich_inputs,
        &mut imports,
    )?;
    let mut params = String::new();
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for field in target.fields.values() {
        let value = if let Some(parameter) = rich_inputs.get(&field.id) {
            let member = &parameter.name;
            if parameter.required && !parameter.optional_filter {
                InsertValue::Parameter(format!("input.{member}()"))
            } else {
                InsertValue::Parameter(format!("input.{member}().orElse(null)"))
            }
        } else if inputs.iter().any(|input| input.id == field.id) {
            let member = &field.names.java_member;
            if field.required {
                InsertValue::Parameter(format!("input.{member}()"))
            } else {
                InsertValue::Parameter(format!("input.{member}().orElse(null)"))
            }
        } else if let Some(value) = resolved_values.get(&field.id) {
            InsertValue::Parameter(value.clone())
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
                "{}.TimeOrderedUuid",
                model.project.package_for("domain")
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
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public {} execute({context}{port_type}.Input input) {{\n{resolutions}        JdbcClient.StatementSpec statement = jdbc.sql(\"{insert} returning {returning}\");\n{params}        return statement.query({}.class).single();\n    }}\n}}",
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

fn local_parameters<'a>(
    command: &'a Command,
    target: &Entity,
) -> Result<BTreeMap<jails_model::FieldId, &'a OperationParameter>, CompileError> {
    let mut parameters = BTreeMap::new();
    for parameter in &command.semantics.parameters {
        let ParameterSource::Field(field) = &parameter.source else {
            continue;
        };
        if field.entity != target.id {
            continue;
        }
        if parameters.insert(field.field.clone(), parameter).is_some() {
            return Err(CompileError::new(format!(
                "canonical command supplies field `{}` from more than one parameter",
                field.field
            )));
        }
    }
    Ok(parameters)
}

fn lower_resolutions(
    model: &AppModel,
    operation: &Operation,
    command: &Command,
    target: &Entity,
    local_parameters: &BTreeMap<jails_model::FieldId, &OperationParameter>,
    imports: &mut BTreeSet<String>,
) -> Result<(BTreeMap<jails_model::FieldId, String>, String), CompileError> {
    let mut values = BTreeMap::new();
    let mut statements = String::new();
    for (position, resolution) in command.semantics.resolutions.iter().enumerate() {
        let target_field = target.fields.get(&resolution.target).ok_or_else(|| {
            CompileError::new(format!(
                "linked command `{}` references missing resolve target `{}`",
                operation.label, resolution.target
            ))
        })?;
        if local_parameters.contains_key(&resolution.target)
            || command
                .semantics
                .assignments
                .iter()
                .any(|assignment| assignment.field == resolution.target)
        {
            return Err(CompileError::new(format!(
                "canonical command `{}` supplies resolved field `{}` from more than one source\n       fix: remove its direct parameter or constant assignment",
                operation.label, target_field.label
            )));
        }
        let remote = model
            .entities
            .get(&resolution.remote_entity)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "linked command `{}` references missing resolve entity `{}`",
                    operation.label, resolution.remote_entity
                ))
            })?;
        let remote_value = remote.fields.get(&resolution.remote_value).ok_or_else(|| {
            CompileError::new(format!(
                "linked command `{}` references missing resolve value `{}`",
                operation.label, resolution.remote_value
            ))
        })?;
        let remote_lookup = remote
            .fields
            .get(&resolution.remote_lookup)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "linked command `{}` references missing resolve lookup `{}`",
                    operation.label, resolution.remote_lookup
                ))
            })?;
        if target_field.ty != remote_value.ty {
            return Err(CompileError::new(format!(
                "canonical command `{}` resolves `{}` from incompatible field `{}.{}`\n       fix: resolve from a field with the same logical type",
                operation.label, target_field.label, remote.label, remote_value.label
            )));
        }
        if !unique_lookup(remote, remote_lookup) {
            return Err(CompileError::new(format!(
                "canonical command `{}` resolves through non-unique field `{}.{}`\n       fix: declare that lookup field primary or unique",
                operation.label, remote.label, remote_lookup.label
            )));
        }
        let parameter = command
            .semantics
            .parameters
            .iter()
            .find(|parameter| parameter.name == resolution.parameter)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "linked command `{}` references missing resolve parameter `{}`",
                    operation.label, resolution.parameter
                ))
            })?;
        let parameter_type = parameter_type(model, parameter)?;
        if parameter_type != &remote_lookup.ty {
            return Err(CompileError::new(format!(
                "canonical command `{}` resolves through parameter `{}` with the wrong type\n       fix: use a parameter matching `{}.{}`",
                operation.label, parameter.name, remote.label, remote_lookup.label
            )));
        }
        if !parameter.required || parameter.optional_filter {
            return Err(CompileError::new(format!(
                "canonical command `{}` uses optional resolve parameter `{}`\n       fix: make the lookup parameter required",
                operation.label, parameter.name
            )));
        }
        let java = java_type(target_field, imports);
        let variable = format!("resolved_{}", target_field.names.java_member);
        let sql_parameter = format!("resolve_{}_{}", target_field.names.sql_column, position);
        statements.push_str(&format!(
            "        {java} {variable} = jdbc.sql(\"select {} from {} where {} = :{sql_parameter}\")\n                .param(\"{sql_parameter}\", input.{}())\n                .query({java}.class)\n                .single();\n",
            remote_value.names.sql_column,
            remote.names.sql_table,
            remote_lookup.names.sql_column,
            parameter.name,
        ));
        if values.insert(resolution.target.clone(), variable).is_some() {
            return Err(CompileError::new(format!(
                "canonical command `{}` resolves field `{}` more than once",
                operation.label, target_field.label
            )));
        }
    }
    Ok((values, statements))
}

fn parameter_type<'a>(
    model: &'a AppModel,
    parameter: &'a OperationParameter,
) -> Result<&'a TypeRef, CompileError> {
    match &parameter.source {
        ParameterSource::Typed(ty) => Ok(ty),
        ParameterSource::Field(field) => model
            .entities
            .get(&field.entity)
            .and_then(|entity| entity.fields.get(&field.field))
            .map(|field| &field.ty)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "linked operation parameter `{}` references a missing field",
                    parameter.name
                ))
            }),
    }
}

fn unique_lookup(entity: &Entity, field: &Field) -> bool {
    field.primary_key
        || field.unique
        || entity.constraints.values().any(|constraint| {
            matches!(
                constraint.kind,
                ConstraintKind::PrimaryKey | ConstraintKind::Unique
            ) && constraint.fields.as_slice() == std::slice::from_ref(&field.id)
        })
}
