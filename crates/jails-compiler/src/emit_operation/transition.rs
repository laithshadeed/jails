use super::{
    assignment_sql_value, context_parameter, context_value, java_string, operation_file,
    resolve_fields, scopes, stored_entity,
};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Operation, OperationKind, Transition};
use std::collections::BTreeSet;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    transition: &Transition,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let target = stored_entity(model, operation, &transition.on, "transition")?;
    let inputs = resolve_fields(operation, target, &transition.fields, "input")?;
    let sets = resolve_fields(operation, target, &transition.sets, "set")?;
    let constant_sets = transition
        .semantics
        .assignments
        .iter()
        .map(|assignment| {
            target
                .fields
                .get(&assignment.field)
                .map(|field| (field, &assignment.value))
                .ok_or_else(|| {
                    CompileError::new(format!(
                        "linked transition `{}` references missing assignment field `{}`",
                        operation.label, assignment.field
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if sets.is_empty() && constant_sets.is_empty() {
        return Err(CompileError::new(format!(
            "canonical transition `{}` changes no fields\n       fix: declare at least one field in `update` or a constant `set` statement",
            operation.label
        )));
    }
    if let Some((field, _)) = constant_sets
        .iter()
        .find(|(field, _)| sets.iter().any(|set| set.id == field.id))
    {
        return Err(CompileError::new(format!(
            "canonical transition `{}` supplies field `{}` from both input and a constant assignment\n       fix: remove the field from `update` or remove its `set` statement",
            operation.label, field.label
        )));
    }
    for field in &sets {
        if !inputs.iter().any(|input| input.id == field.id) {
            return Err(CompileError::new(format!(
                "canonical transition `{}` sets `{}` without carrying it as input\n       fix: add `{}` to `fields` or remove it from `sets`",
                operation.label, field.label, field.label
            )));
        }
    }
    let primary_key = primary_key(target)?;
    if sets.iter().any(|field| field.id == primary_key.id)
        || constant_sets
            .iter()
            .any(|(field, _)| field.id == primary_key.id)
    {
        return Err(CompileError::new(format!(
            "canonical transition `{}` attempts to rewrite primary key `{}`\n       fix: remove the primary key from `sets`",
            operation.label, primary_key.label
        )));
    }
    let guards = inputs
        .iter()
        .copied()
        .filter(|input| !sets.iter().any(|field| field.id == input.id))
        .collect::<Vec<_>>();
    let port_type = with_suffix(&operation.names.java_type, "Transition");
    let type_name = format!("Jdbc{port_type}");
    let mut imports = BTreeSet::from([
        format!(
            "{}.application.transitions.{port_type}",
            model.project.base_package
        ),
        domain_import(model, target),
        "java.util.ArrayList".to_string(),
        "java.util.List".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
    ]);
    let key_type = java_type(primary_key, &mut imports);
    let context = context_parameter(model, target, &mut imports);
    let scope_fields = scopes(target);
    let (event_member, event_parameter, event_assignment, result) = if let Some(event_id) =
        &transition.yields
    {
        let yielded = model.operations.get(event_id).ok_or_else(|| {
            CompileError::new(format!(
                "linked transition `{}` references missing event `{event_id}`",
                operation.label
            ))
        })?;
        let OperationKind::Event(event) = &yielded.kind else {
            return Err(CompileError::new(format!(
                "linked transition `{}` yields non-event operation `{}`",
                operation.label, yielded.label
            )));
        };
        if event.on.as_ref().is_some_and(|entity| entity != &target.id) {
            return Err(CompileError::new(format!(
                "canonical transition `{}` yields event `{}` from another entity\n       fix: yield an event projected from `{}`",
                operation.label, yielded.label, target.label
            )));
        }
        let event_type = with_suffix(&yielded.names.java_type, "Event");
        imports.extend([
            format!("{}.domain.events.{event_type}", model.project.base_package),
            "org.springframework.context.ApplicationEventPublisher".to_string(),
        ]);
        let arguments = event
            .fields
            .iter()
            .map(|field_id| {
                target
                    .fields
                    .get(field_id)
                    .map(|field| format!("result.{}()", field.names.java_member))
                    .ok_or_else(|| {
                        CompileError::new(format!(
                            "linked event `{}` references missing field `{field_id}`",
                            yielded.label
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        (
            "\n    private final ApplicationEventPublisher events;",
            ", ApplicationEventPublisher events",
            "\n        this.events = events;",
            format!(
                "        var result = statement.query({}.class).single();\n        events.publishEvent(new {event_type}({arguments}));\n        return result;",
                target.names.java_type
            ),
        )
    } else {
        (
            "",
            "",
            "",
            format!(
                "        return statement.query({}.class).single();",
                target.names.java_type
            ),
        )
    };
    let mut assignments = sets
        .iter()
        .map(|field| format!("{} = :{}", field.names.sql_column, field.names.sql_column))
        .collect::<Vec<_>>();
    assignments.extend(
        constant_sets
            .iter()
            .map(|(field, value)| {
                assignment_sql_value(operation, field, value)
                    .map(|value| format!("{} = {value}", field.names.sql_column))
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    assignments.extend(target.fields.values().filter_map(|field| {
        if field.semantics.version {
            Some(format!(
                "{} = {} + 1",
                field.names.sql_column, field.names.sql_column
            ))
        } else if field.semantics.updated {
            Some(format!("{} = current_timestamp", field.names.sql_column))
        } else {
            None
        }
    }));
    let assignments = assignments.join(", ");
    let required_guards = guards.iter().filter(|field| field.required).map(|field| {
        format!(
            "{} = :guard_{}",
            field.names.sql_column, field.names.sql_column
        )
    });
    let predicate_seed = std::iter::once(format!("{} = :id", primary_key.names.sql_column))
        .chain(required_guards)
        .chain(scope_fields.iter().map(|field| {
            format!(
                "{} = :scope_{}",
                field.names.sql_column, field.names.sql_column
            )
        }))
        .map(|predicate| java_string(&predicate))
        .collect::<Vec<_>>()
        .join(", ");
    let optional_guards = guards
        .iter()
        .filter(|field| !field.required)
        .map(|field| {
            format!(
                "        if (input.{}().isPresent()) {{\n            predicates.add(\"{} = :guard_{}\");\n        }}\n",
                field.names.java_member, field.names.sql_column, field.names.sql_column
            )
        })
        .collect::<String>();
    let set_params = sets
        .iter()
        .map(|field| {
            let value = if field.required {
                format!("input.{}()", field.names.java_member)
            } else {
                format!("input.{}().orElse(null)", field.names.java_member)
            };
            format!(
                "        statement = statement.param(\"{}\", {value});\n",
                field.names.sql_column
            )
        })
        .collect::<String>();
    let guard_params = guards
        .iter()
        .map(|field| {
            if field.required {
                format!(
                    "        statement = statement.param(\"guard_{}\", input.{}());\n",
                    field.names.sql_column, field.names.java_member
                )
            } else {
                format!(
                    "        if (input.{}().isPresent()) {{\n            statement = statement.param(\"guard_{}\", input.{}().orElseThrow());\n        }}\n",
                    field.names.java_member, field.names.sql_column, field.names.java_member
                )
            }
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
    let columns = target
        .fields
        .values()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;{event_member}\n\n    public {type_name}(JdbcClient jdbc{event_parameter}) {{\n        this.jdbc = jdbc;{event_assignment}\n    }}\n\n    @Override\n    @Transactional\n    public {} execute({context}{key_type} id, {port_type}.Input input) {{\n        var predicates = new ArrayList<>(List.of({predicate_seed}));\n{optional_guards}        var sql = \"update {} set {assignments} where \" + String.join(\" and \", predicates) + \" returning {columns}\";\n        JdbcClient.StatementSpec statement = jdbc.sql(sql);\n        statement = statement.param(\"id\", id);\n{set_params}{guard_params}{scope_params}{result}\n    }}\n}}",
        target.names.java_type, target.names.sql_table
    );
    operation_file(
        model,
        capability_id,
        operation,
        target,
        &type_name,
        "transition",
        "capability-db-transition",
        imports,
        body,
    )
}
