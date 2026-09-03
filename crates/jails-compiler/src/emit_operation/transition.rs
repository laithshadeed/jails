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
use crate::Diagnostic;
use crate::emit_java::{domain_import, java_type, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, Operation, Package, Transition};
use std::collections::BTreeSet;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    transition: &Transition,
) -> Result<(ProjectPath, RenderedFile), Diagnostic> {
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
                    crate::refuse::unlinked(
                        format!("$.operations.{}", operation.label),
                        format!(
                            "linked transition `{}` references missing assignment field `{}`",
                            operation.label, assignment.field
                        ),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if sets.is_empty() && constant_sets.is_empty() {
        return Err(Diagnostic::new(
            "compile-transition-changes-nothing",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical transition `{}` changes no fields",
                operation.label
            ),
            "declare at least one field in `update` or a constant `set` statement",
        ));
    }
    if let Some((field, _)) = constant_sets
        .iter()
        .find(|(field, _)| sets.iter().any(|set| set.id == field.id))
    {
        return Err(Diagnostic::new(
            "compile-transition-field-supplied-twice",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical transition `{}` supplies field `{}` from both input and a constant assignment",
                operation.label, field.label
            ),
            "remove the field from `update` or remove its `set` statement",
        ));
    }
    for field in &sets {
        if !inputs.iter().any(|input| input.id == field.id) {
            return Err(Diagnostic::new(
                "compile-transition-sets-uncarried-field",
                format!("$.operations.{}", operation.label),
                format!(
                    "canonical transition `{}` sets `{}` without carrying it as input",
                    operation.label, field.label
                ),
                format!("add `{}` to `fields` or remove it from `sets`", field.label),
            ));
        }
    }
    if sets.iter().any(|field| field.id == primary_key.id)
        || constant_sets
            .iter()
            .any(|(field, _)| field.id == primary_key.id)
    {
        return Err(Diagnostic::new(
            "compile-transition-rewrites-primary-key",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical transition `{}` attempts to rewrite primary key `{}`",
                operation.label, primary_key.label
            ),
            "remove the primary key from `sets`",
        ));
    }
    // JDL v1 §12.4: parameters in `select` identify the row, parameters in
    // `update` provide new values, and every remaining entity parameter is an
    // equality guard. The selector is part of this subtraction, or a declared
    // `select [id]` renders as a guard *and* as an update.
    // **The precondition is not a guard, because it does not come from the
    // body.** It arrives as `If-Match`, so `execute` takes it as its own
    // argument and the SQL binds `:expected_version` -- an optional one
    // through `coalesce`, which is one statement rather than two and is also
    // what gives a null parameter a type PostgreSQL can compare with `=`.
    let precondition = crate::emit_java::precondition(target, transition);
    let guards = inputs
        .iter()
        .copied()
        .filter(|input| {
            !sets.iter().any(|field| field.id == input.id)
                && !selector.iter().any(|field| field.id == input.id)
                && precondition
                    .as_ref()
                    .is_none_or(|version| version.field.id != input.id)
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
    let key_type = java_type(selector[0], &mut imports);
    // The port names this parameter after the component it selects on, and an
    // override that renamed it would read as a different key to anyone
    // comparing the two.
    let key_member = &selector[0].names.java_member;
    let context = context_parameter(model, target, &mut imports);
    let scope_fields = scopes(target);
    let publications = super::publications(
        model,
        operation,
        target,
        &transition.semantics.emits,
        &mut imports,
    )?;
    // **Zero rows has two causes and they are different answers.** A stated
    // `If-Match` that no longer matches is a 412 and a row that is not there
    // is a 404; `.single()` cannot tell them apart and raises one unclassified
    // failure that Spring reports as a 500 -- which alerting pages on and
    // client libraries retry, and the retry can never succeed. So the update
    // runs `.optional()` and, only when it comes back empty, asks whether the
    // key exists at all. Two statements, one `@Transactional` method, and the
    // second one runs on the failure path only.
    //
    // Both exceptions are Spring's own, from `spring-dao`, which is on the
    // classpath the moment the JDBC starter is: the `api` capability's advice
    // maps them, and a hand-written adapter that raises the same pair gets the
    // same answer for free.
    let outcome = precondition.as_ref().map(|_| {
        imports.insert("org.springframework.dao.EmptyResultDataAccessException".to_string());
        imports.insert("org.springframework.dao.OptimisticLockingFailureException".to_string());
        format!(
            "        var applied = statement.query({}.class).optional();\n        if (applied.isEmpty()) {{\n            throw jdbc.sql(\"select 1 from {} where {} = :{}\")\n                    .param(\"{}\", {})\n                    .query(Integer.class)\n                    .optional()\n                    .<RuntimeException>map(row -> new OptimisticLockingFailureException(\n                            \"the resource has changed since the version you sent\"))\n                    .orElseGet(() -> new EmptyResultDataAccessException(1));\n        }}\n",
            target.names.java_type,
            target.names.sql_table,
            selector[0].names.sql_column,
            selector[0].names.sql_column,
            selector[0].names.sql_column,
            key_member,
        )
    });
    let (unwrap, take) = match &outcome {
        Some(_) => ("applied.get()", "applied.get()"),
        None => (
            "statement.query(_TYPE_.class).single()",
            "statement.query(_TYPE_.class).single()",
        ),
    };
    let unwrap = unwrap.replace("_TYPE_", &target.names.java_type);
    let take = take.replace("_TYPE_", &target.names.java_type);
    let probe = outcome.unwrap_or_default();
    let (event_member, event_parameter, event_assignment, result) = if publications.is_empty() {
        ("", "", "", format!("{probe}        return {unwrap};"))
    } else {
        (
            "\n    private final ApplicationEventPublisher events;",
            ", ApplicationEventPublisher events",
            "\n        this.events = events;",
            format!(
                "{probe}        var result = {take};{}\n        return result;",
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
    // **The parameter is named after the column it constrains.** `:id` is the
    // right name only when the selector *is* the primary key: a transition
    // selecting on `user_id` would render `user_id = :id`, which is correct SQL
    // and reads as a mistake -- and a reader adding a predicate of their own
    // below has no way to tell which binding `:id` refers to.
    let selector_param = selector[0].names.sql_column.as_str();
    let expected_predicate = precondition.as_ref().map(|version| {
        let column = &version.field.names.sql_column;
        if version.required {
            format!("{column} = :expected_version")
        } else {
            format!("{column} = coalesce(:expected_version, {column})")
        }
    });
    let predicate_seed = std::iter::once(format!("{selector_param} = :{selector_param}"))
        .chain(expected_predicate)
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
                crate::emit_sql::bound_value(
                    model,
                    field,
                    &format!("input.{}()", field.names.java_member),
                    &mut imports,
                )
            } else {
                crate::emit_sql::optional_bound_value(
                    model,
                    field,
                    &format!("input.{}()", field.names.java_member),
                    &mut imports,
                )
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
                    "        statement = statement.param(\"guard_{}\", {});\n",
                    field.names.sql_column,
                    crate::emit_sql::bound_value(
                        model,
                        field,
                        &format!("input.{}()", field.names.java_member),
                        &mut imports
                    )
                )
            } else {
                format!(
                    "        if (input.{}().isPresent()) {{\n            statement = statement.param(\"guard_{}\", {});\n        }}\n",
                    field.names.java_member,
                    field.names.sql_column,
                    crate::emit_sql::bound_value(
                        model,
                        field,
                        &format!("input.{}().orElseThrow()", field.names.java_member),
                        &mut imports
                    )
                )
            }
        })
        .collect::<String>();
    let expected_param = precondition.as_ref().map_or_else(String::new, |_| {
        "        statement = statement.param(\"expected_version\", expectedVersion);\n".to_string()
    });
    let expected_argument = precondition
        .as_ref()
        .map(|version| version.parameter(&mut imports))
        .unwrap_or_default();
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
        "@Repository\npublic class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;{event_member}\n\n    public {type_name}(JdbcClient jdbc{event_parameter}) {{\n        this.jdbc = jdbc;{event_assignment}\n    }}\n\n    @Override\n    @Transactional\n    public {} execute({context}{key_type} {key_member}, {port_type}.Input input{expected_argument}) {{\n        var predicates = new ArrayList<String>(List.of({predicate_seed}));\n{optional_guards}        var sql = \"update {} set {assignments} where \" + String.join(\" and \", predicates) + \" returning {columns}\";\n        JdbcClient.StatementSpec statement = jdbc.sql(sql);\n        statement = statement.param(\"{selector_param}\", {key_member});\n{set_params}{expected_param}{guard_params}{scope_params}{result}\n    }}\n}}",
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
/// JDL v1 §12.4 allows any field list here, and the update statement below
/// binds exactly one -- so a single selector renders whichever field it
/// names, and more than one refuses rather than silently widening the `where`
/// clause to every matching row. Answer exactly or refuse.
///
/// **Any single field, not only the primary key.** `--select userId --path
/// /topics/{userId}/subject` -- a transition keyed on the column its own
/// route carries -- binds one column either way, and nothing about `id` is
/// load-bearing.
fn selector<'a>(
    operation: &Operation,
    target: &'a jails_model::Entity,
    transition: &jails_model::Transition,
    primary_key: &'a jails_model::Field,
) -> Result<Vec<&'a jails_model::Field>, Diagnostic> {
    if transition.semantics.select.is_empty() {
        return Ok(vec![primary_key]);
    }
    let selected = resolve_fields(operation, target, &transition.semantics.select, "select")?;
    if selected.len() != 1 {
        return Err(Diagnostic::new(
            "compile-transition-selector-not-single",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical transition `{}` selects rows by {} fields, and the update statement binds one",
                operation.label,
                selected.len()
            ),
            "select a single field, or eject this adapter and write the statement by hand",
        ));
    }
    Ok(selected)
}

/// The fields this transition writes from its own input.
///
/// An explicit `update` list wins. When it is omitted, JDL v1 §12.4 keeps
/// the familiar CLI shape: every remaining entity parameter is updated,
/// *minus the row selector and minus the compiler-managed fields*. Those two
/// subtractions are the whole of this function, and leaving them out makes
/// `transition Close(id) { select [id] }` report that it rewrites the primary
/// key.
fn updates<'a>(
    operation: &Operation,
    target: &'a jails_model::Entity,
    transition: &jails_model::Transition,
    inputs: &[&'a jails_model::Field],
    selector: &[&jails_model::Field],
) -> Result<Vec<&'a jails_model::Field>, Diagnostic> {
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
