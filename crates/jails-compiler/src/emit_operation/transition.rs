//! The JDBC adapter behind a linked `transition`.
//!
//! A transition is the narrow write: `update ... set ... where <selector>`,
//! where the selector is the entity's primary key plus any guard the
//! declaration named. Both halves are resolved against the model, so a
//! transition cannot update a column the entity does not have and cannot
//! select on one either.
//!
//! **The selector is the whole safety property.** An `update` whose `where`
//! clause silently lost a term still compiles, still runs, and rewrites every
//! row in the table. So the selector is built from typed field references and
//! the primary key is required rather than inferred; there is no path here
//! where an empty selector renders.
//!
//! Constant assignments and input assignments are kept apart deliberately: a
//! declared constant is the model's, an input is the caller's, and rendering
//! them through one list would let a request supply a value the declaration
//! said was fixed.

use super::{
    assignment_sql_value, context_parameter, context_value, java_string, operation_file,
    resolve_fields, scopes, stored_entity,
};
use crate::CompileError;
use crate::emit_java::{domain_import, java_type, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Operation, Package, Transition};
use std::collections::BTreeSet;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    transition: &Transition,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let target = stored_entity(model, operation, &transition.on, "transition")?;
    let inputs = resolve_fields(operation, target, &transition.fields, "input")?;
    let primary_key = primary_key(target)?;
    let selector = selector(operation, target, transition, primary_key)?;
    let sets = updates(operation, target, transition, &inputs, &selector)?;
    let constant_sets = transition
        .semantics
        .assignments
        .iter()
        .map(|assignment| {
            target
                .field(&assignment.field)
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
    // `jdl-sol.md` §12.4: parameters in `select` identify the row, parameters
    // in `update` provide new values, and every remaining entity parameter is
    // an equality guard. The selector was missing from this subtraction, so a
    // declared `select [id]` was rendered as a guard *and* as an update.
    let guards = inputs
        .iter()
        .copied()
        .filter(|input| {
            !sets.iter().any(|field| field.id == input.id)
                && !selector.iter().any(|field| field.id == input.id)
        })
        .collect::<Vec<_>>();
    let port_type = with_suffix(&operation.names.java_type, "Transition");
    let type_name = format!("Jdbc{port_type}");
    let mut imports = BTreeSet::from([
        format!(
            "{}.{port_type}",
            model.project.package_for(Package::ApplicationTransitions)
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
    let publications = super::publications(
        model,
        operation,
        target,
        &transition.semantics.emits,
        &mut imports,
    )?;
    let (event_member, event_parameter, event_assignment, result) = if publications.is_empty() {
        (
            "",
            "",
            "",
            format!(
                "        return statement.query({}.class).single();",
                target.names.java_type
            ),
        )
    } else {
        (
            "\n    private final ApplicationEventPublisher events;",
            ", ApplicationEventPublisher events",
            "\n        this.events = events;",
            format!(
                "        var result = statement.query({}.class).single();{}\n        return result;",
                target.names.java_type,
                publications.concat()
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
    assignments.extend(target.fields.iter().filter_map(|field| {
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
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        // **Not `final`, because `@Transactional` forces a proxy.** Spring Boot
        // defaults `spring.aop.proxy-target-class=true`, so the transaction advice is
        // applied by CGLIB subclassing -- which cannot subclass a final class, and the
        // context fails at startup with "Could not generate CGLIB subclass". The
        // adapter implements its port, so a JDK proxy would do, but making the whole
        // application proxy by interface to keep one `final` is the wrong trade.
        "@Repository\npublic class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;{event_member}\n\n    public {type_name}(JdbcClient jdbc{event_parameter}) {{\n        this.jdbc = jdbc;{event_assignment}\n    }}\n\n    @Override\n    @Transactional\n    public {} execute({context}{key_type} id, {port_type}.Input input) {{\n        var predicates = new ArrayList<>(List.of({predicate_seed}));\n{optional_guards}        var sql = \"update {} set {assignments} where \" + String.join(\" and \", predicates) + \" returning {columns}\";\n        JdbcClient.StatementSpec statement = jdbc.sql(sql);\n        statement = statement.param(\"id\", id);\n{set_params}{guard_params}{scope_params}{result}\n    }}\n}}",
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

/// The fields that identify the row, defaulting to the primary key.
///
/// `jdl-sol.md` §12.4 allows any field list here, but the update statement
/// below binds the key as `:id` and nothing else, so a selector this emitter
/// cannot render refuses rather than silently widening the `where` clause to
/// every matching row. Answer exactly or refuse.
fn selector<'a>(
    operation: &Operation,
    target: &'a jails_model::Entity,
    transition: &jails_model::Transition,
    primary_key: &'a jails_model::Field,
) -> Result<Vec<&'a jails_model::Field>, CompileError> {
    if transition.semantics.select.is_empty() {
        return Ok(vec![primary_key]);
    }
    let selected = resolve_fields(operation, target, &transition.semantics.select, "select")?;
    if selected.len() != 1 || selected[0].id != primary_key.id {
        return Err(CompileError::new(format!(
            "canonical transition `{}` selects rows by a field other than primary key `{}`\n       fix: select the primary key, or eject this adapter and write the statement by hand",
            operation.label, primary_key.label
        )));
    }
    Ok(selected)
}

/// The fields this transition writes from its own input.
///
/// An explicit `update` list wins. When it is omitted, `jdl-sol.md` §12.4
/// keeps the familiar CLI shape: every remaining entity parameter is updated,
/// *minus the row selector and minus the compiler-managed fields*. Those two
/// subtractions are the whole of this function, and leaving them out is what
/// made `transition Close(id) { select [id] }` report that it rewrites the
/// primary key.
fn updates<'a>(
    operation: &Operation,
    target: &'a jails_model::Entity,
    transition: &jails_model::Transition,
    inputs: &[&'a jails_model::Field],
    selector: &[&jails_model::Field],
) -> Result<Vec<&'a jails_model::Field>, CompileError> {
    if !transition.semantics.update.is_empty() {
        return resolve_fields(operation, target, &transition.semantics.update, "update");
    }
    Ok(inputs
        .iter()
        .copied()
        .filter(|field| {
            !selector.iter().any(|selected| selected.id == field.id)
                && !field.semantics.version
                && !field.semantics.updated
                && field.semantics.scope.is_none()
        })
        .collect())
}
