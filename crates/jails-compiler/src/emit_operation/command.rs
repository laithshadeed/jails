//! The JDBC adapter behind a linked `command`.
//!
//! A command is the write side: one `insert ... returning`, built from the
//! entity's own column projection so the insert list, the bind list and the row
//! mapper cannot drift apart. Everything it needs is in the model — which
//! fields are inputs, which are assigned constants, which come from the request
//! context, and which are minted — so nothing here guesses a value the reader
//! did not declare.
//!
//! **Emitting is refusing, quite often.** A field the operation names and the
//! entity does not, an entity with no primary key, an assignment whose value
//! has no SQL rendering: each is an error here rather than Java that fails
//! later, because the compiler is the last place that still knows *why* the
//! shape was asked for. A generated adapter that compiles and does the wrong
//! thing is the one outcome worse than a refusal.
//!
//! The order the insert value for a field is chosen in is the interesting
//! part, and it is a ladder rather than a table: a declared assignment wins,
//! then a `@scope` field (which comes from the request context, never from the
//! caller's body), then `updated`, then a `uuid7` default the adapter mints
//! client-side, then any other default, which is *omitted* so the database
//! supplies it. Reordering those arms silently changes where a value comes
//! from.
//!
//! Event publication is not this module's — `emit_operation::publications`
//! decides whether an emitted event is published in-process or staged in an
//! outbox, because that answer belongs to the operation as a whole rather than
//! to the statement.

use super::{
    assignment_sql_value, context_parameter, context_value, operation_file, resolve_fields,
    stored_entity,
};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{
    AppModel, Command, ConstraintKind, Entity, Field, Operation, OperationParameter, Package,
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
            model.project.package_for(Package::ApplicationCommands),
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
    for field in target.fields.iter() {
        let value = if let Some(parameter) = rich_inputs.get(&field.id) {
            let member = crate::emit_java::parameter_member(parameter);
            if parameter.required && !parameter.optional_filter {
                InsertValue::Parameter(crate::emit_sql::bound_value(
                    model,
                    field,
                    &format!("input.{member}()"),
                    &mut imports,
                ))
            } else {
                InsertValue::Parameter(crate::emit_sql::optional_bound_value(
                    model,
                    field,
                    &format!("input.{member}()"),
                    &mut imports,
                ))
            }
        } else if inputs.iter().any(|input| input.id == field.id) {
            let member = &field.names.java_member;
            if field.required {
                InsertValue::Parameter(crate::emit_sql::bound_value(
                    model,
                    field,
                    &format!("input.{member}()"),
                    &mut imports,
                ))
            } else {
                InsertValue::Parameter(crate::emit_sql::optional_bound_value(
                    model,
                    field,
                    &format!("input.{member}()"),
                    &mut imports,
                ))
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
                model.project.package_for(Package::Domain)
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
        .iter()
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
    // A command publishes what it declares, the same way a transition does.
    // `CommandSemantics::emits` was linked and read by nobody, so `command
    // Create(...) { emit TaskCreated }` generated the payload record and an
    // adapter that never mentioned it.
    let publications = super::publications(
        model,
        operation,
        target,
        &command.semantics.emits,
        &mut imports,
    )?;
    // What the collaborator *is* follows from how the command delivers. A
    // direct publication takes Spring's publisher; an outbox stages into its
    // own store, and the staging insert has to be in the statement's
    // transaction or the whole guarantee is gone -- so `@Transactional` is
    // part of the same decision rather than a separate one somebody can
    // forget.
    let staged = super::outbox::delivery(operation) == jails_model::Delivery::Outbox;
    let (collaborator, method_annotation, result) = if publications.is_empty() {
        (
            None,
            "",
            format!(
                "        return statement.query({}.class).single();",
                target.names.java_type
            ),
        )
    } else {
        (
            Some(if staged {
                (format!("Jdbc{}Outbox", operation.names.java_type), "outbox")
            } else {
                ("ApplicationEventPublisher".to_string(), "events")
            }),
            if staged { "    @Transactional\n" } else { "" },
            format!(
                "        var result = statement.query({}.class).single();{}\n        return result;",
                target.names.java_type,
                publications.concat()
            ),
        )
    };
    let (member, parameter, assignment) = match &collaborator {
        None => (String::new(), String::new(), String::new()),
        Some((ty, name)) => (
            format!("\n    private final {ty} {name};"),
            format!(", {ty} {name}"),
            format!("\n        this.{name} = {name};"),
        ),
    };
    let body = format!(
        // **Not `final`, because `@Transactional` forces a proxy.** Spring Boot
        // defaults `spring.aop.proxy-target-class=true`, so the transaction advice is
        // applied by CGLIB subclassing -- which cannot subclass a final class, and the
        // context fails at startup with "Could not generate CGLIB subclass". The
        // adapter implements its port, so a JDK proxy would do, but making the whole
        // application proxy by interface to keep one `final` is the wrong trade.
        "@Repository\npublic class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;{member}\n\n    public {type_name}(JdbcClient jdbc{parameter}) {{\n        this.jdbc = jdbc;{assignment}\n    }}\n\n    @Override\n{method_annotation}    public {} execute({context}{port_type}.Input input) {{\n{resolutions}        JdbcClient.StatementSpec statement = jdbc.sql(\"{insert} returning {returning}\");\n{params}{result}\n    }}\n}}",
        target.names.java_type
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
        let target_field = target.field(&resolution.target).ok_or_else(|| {
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
        let remote_value = remote.field(&resolution.remote_value).ok_or_else(|| {
            CompileError::new(format!(
                "linked command `{}` references missing resolve value `{}`",
                operation.label, resolution.remote_value
            ))
        })?;
        let remote_lookup = remote.field(&resolution.remote_lookup).ok_or_else(|| {
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
            crate::emit_java::parameter_member(parameter),
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
            .and_then(|entity| entity.field(&field.field))
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
