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
use crate::Diagnostic;
use crate::emit_java::{domain_import, primary_key, with_suffix};
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
) -> Result<(ProjectPath, RenderedFile), Diagnostic> {
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
            return Err(Diagnostic::new(
                "compile-command-field-supplied-twice",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` supplies field `{}` from both input and a constant assignment",
                    operation.label, assignment.field
                ),
                "remove the field parameter or its `set` statement",
            ));
        }
    }
    let resolved = lower_resolutions(model, operation, command, target, &rich_inputs)?;
    let resolve_params = resolved.params.clone();
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
        } else if let Some(value) = resolved.values.get(&field.id) {
            InsertValue::Expression(value.clone())
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
        } else if field.semantics.version {
            // **An optimistic-lock column is never the caller's to set**, and
            // the schema already says what it starts at: the column is
            // `bigint default 0 not null`, so the insert leaves it out rather
            // than restating the default in a second place. Asking a command
            // to carry it would let a caller choose the version their own next
            // write is checked against, which is the concurrency control
            // removed rather than used.
            InsertValue::Omitted
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
            return Err(Diagnostic::new(
                "compile-command-required-field-unconstructable",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` cannot construct required field `{}`",
                    operation.label, field.label
                ),
                format!(
                    "carry `{}` in the command or declare a typed field default",
                    field.label
                ),
            ));
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
    } else if resolved.from.is_empty() {
        format!(
            "insert into {} ({}) values ({})",
            target.names.sql_table,
            columns.join(", "),
            values.join(", ")
        )
    } else {
        format!(
            "insert into {} ({}) select {} from {} where {}",
            target.names.sql_table,
            columns.join(", "),
            values.join(", "),
            resolved.from.join(", "),
            resolved.conditions.join(" and ")
        )
    };
    let port_type = with_suffix(&operation.names.java_type, "Command");
    let type_name = format!("Jdbc{port_type}");
    // A command publishes what it declares, the same way a transition does:
    // `command Create(...) { emit TaskCreated }` gets the payload record and
    // an adapter that publishes it.
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
    // **A resolved insert writes nothing when the parent is not there**, so
    // the row it answers with is optional and the port says so. Without a
    // resolution the statement always writes exactly one row.
    let optional = !command.semantics.resolutions.is_empty();
    let (answer, take) = if optional {
        imports.insert("java.util.Optional".to_string());
        (format!("Optional<{}>", target.names.java_type), "optional")
    } else {
        (target.names.java_type.clone(), "single")
    };
    let (collaborator, method_annotation, result) = if publications.is_empty() {
        (
            None,
            "",
            format!(
                "        return statement.query({}.class).{take}();",
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
                "        var result = statement.query({}.class).{take}();{}\n        return result;",
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
        "@Repository\npublic class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;{member}\n\n    public {type_name}(JdbcClient jdbc{parameter}) {{\n        this.jdbc = jdbc;{assignment}\n    }}\n\n    @Override\n{method_annotation}    public {answer} execute({context}{port_type}.Input input) {{\n        JdbcClient.StatementSpec statement = jdbc.sql(\"{insert} returning {returning}\");\n{resolve_params}{params}{result}\n    }}\n}}"
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
) -> Result<BTreeMap<jails_model::FieldId, &'a OperationParameter>, Diagnostic> {
    let mut parameters = BTreeMap::new();
    for parameter in &command.semantics.parameters {
        let ParameterSource::Field(field) = &parameter.source else {
            continue;
        };
        if field.entity != target.id {
            continue;
        }
        if parameters.insert(field.field.clone(), parameter).is_some() {
            return Err(Diagnostic::without_a_fix(
                "compile-command-field-from-many-parameters",
                format!("$.entities.{}", target.label),
                format!(
                    "canonical command supplies field `{}` from more than one parameter",
                    field.field
                ),
            ));
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
) -> Result<Resolutions, Diagnostic> {
    let mut resolved = Resolutions::default();
    for (position, resolution) in command.semantics.resolutions.iter().enumerate() {
        let target_field = target.field(&resolution.target).ok_or_else(|| {
            crate::refuse::unlinked(
                format!("$.operations.{}", operation.label),
                format!(
                    "linked command `{}` references missing resolve target `{}`",
                    operation.label, resolution.target
                ),
            )
        })?;
        if local_parameters.contains_key(&resolution.target)
            || command
                .semantics
                .assignments
                .iter()
                .any(|assignment| assignment.field == resolution.target)
        {
            return Err(Diagnostic::new(
                "compile-command-resolved-field-supplied-twice",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` supplies resolved field `{}` from more than one source",
                    operation.label, target_field.label
                ),
                "remove its direct parameter or constant assignment",
            ));
        }
        let remote = model
            .entities
            .get(&resolution.remote_entity)
            .ok_or_else(|| {
                crate::refuse::unlinked(
                    format!("$.operations.{}", operation.label),
                    format!(
                        "linked command `{}` references missing resolve entity `{}`",
                        operation.label, resolution.remote_entity
                    ),
                )
            })?;
        let remote_value = remote.field(&resolution.remote_value).ok_or_else(|| {
            crate::refuse::unlinked(
                format!("$.operations.{}", operation.label),
                format!(
                    "linked command `{}` references missing resolve value `{}`",
                    operation.label, resolution.remote_value
                ),
            )
        })?;
        let remote_lookup = remote.field(&resolution.remote_lookup).ok_or_else(|| {
            crate::refuse::unlinked(
                format!("$.operations.{}", operation.label),
                format!(
                    "linked command `{}` references missing resolve lookup `{}`",
                    operation.label, resolution.remote_lookup
                ),
            )
        })?;
        if target_field.ty != remote_value.ty {
            return Err(Diagnostic::new(
                "compile-command-resolve-type-mismatch",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` resolves `{}` from incompatible field `{}.{}`",
                    operation.label, target_field.label, remote.label, remote_value.label
                ),
                "resolve from a field with the same logical type",
            ));
        }
        if !unique_lookup(remote, remote_lookup) {
            return Err(Diagnostic::new(
                "compile-command-resolve-lookup-not-unique",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` resolves through non-unique field `{}.{}`",
                    operation.label, remote.label, remote_lookup.label
                ),
                "declare that lookup field primary or unique",
            ));
        }
        let parameter = command
            .semantics
            .parameters
            .iter()
            .find(|parameter| parameter.name == resolution.parameter)
            .ok_or_else(|| {
                crate::refuse::unlinked(
                    format!("$.operations.{}", operation.label),
                    format!(
                        "linked command `{}` references missing resolve parameter `{}`",
                        operation.label, resolution.parameter
                    ),
                )
            })?;
        let parameter_type = parameter_type(model, parameter)?;
        if parameter_type != &remote_lookup.ty {
            return Err(Diagnostic::new(
                "compile-command-resolve-parameter-type",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` resolves through parameter `{}` with the wrong type",
                    operation.label, parameter.name
                ),
                format!(
                    "use a parameter matching `{}.{}`",
                    remote.label, remote_lookup.label
                ),
            ));
        }
        if !parameter.required || parameter.optional_filter {
            return Err(Diagnostic::new(
                "compile-command-resolve-parameter-optional",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` uses optional resolve parameter `{}`",
                    operation.label, parameter.name
                ),
                "make the lookup parameter required",
            ));
        }
        // **One statement, not two.** Reading the parent's key and then
        // inserting leaves a window in which the parent is deleted between
        // them, and a `select` inside the `insert` closes it: the row is
        // written from the parent's own row or not at all.
        let table = remote.names.sql_table.clone();
        let sql_parameter = format!("resolve_{}_{}", target_field.names.sql_column, position);
        if resolved
            .values
            .insert(
                resolution.target.clone(),
                format!("{table}.{}", remote_value.names.sql_column),
            )
            .is_some()
        {
            return Err(Diagnostic::without_a_fix(
                "compile-command-field-resolved-twice",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical command `{}` resolves field `{}` more than once",
                    operation.label, target_field.label
                ),
            ));
        }
        if !resolved.from.contains(&table) {
            resolved.from.push(table.clone());
        }
        resolved.conditions.push(format!(
            "{table}.{} = :{sql_parameter}",
            remote_lookup.names.sql_column
        ));
        resolved.params.push_str(&format!(
            "        statement = statement.param(\"{sql_parameter}\", input.{}());\n",
            crate::emit_java::parameter_member(parameter),
        ));
    }
    Ok(resolved)
}

/// What a command's `resolve` declarations contribute to its one statement.
#[derive(Default)]
struct Resolutions {
    /// The SQL expression each resolved column is selected from.
    values: BTreeMap<jails_model::FieldId, String>,
    /// The parent tables the `select` reads.
    from: Vec<String>,
    /// One equality per lookup, all of which must hold.
    conditions: Vec<String>,
    /// The bindings for those lookups.
    params: String,
}

fn parameter_type<'a>(
    model: &'a AppModel,
    parameter: &'a OperationParameter,
) -> Result<&'a TypeRef, Diagnostic> {
    match &parameter.source {
        ParameterSource::Typed(ty) => Ok(ty),
        ParameterSource::Field(field) => model
            .entities
            .get(&field.entity)
            .and_then(|entity| entity.field(&field.field))
            .map(|field| &field.ty)
            .ok_or_else(|| {
                crate::refuse::unlinked(
                    "$.operations",
                    format!(
                        "linked operation parameter `{}` references a missing field",
                        parameter.name
                    ),
                )
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
