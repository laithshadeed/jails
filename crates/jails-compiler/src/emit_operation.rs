//! Executable JDBC adapters for canonical operations.

mod command;
pub(crate) mod outbox;
mod query;
mod transition;

use crate::CompileError;
use crate::emit_java::{JAVA_ROOT, entity, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, Facet, Field, Operation, OperationKind, Package, StableId,
    TypeRef, Value,
};
use std::collections::BTreeSet;

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        let lowered = match &operation.kind {
            OperationKind::Command(spec) => {
                command::lower(model, capability.id.as_str(), operation, spec)?
            }
            OperationKind::Query(spec) => {
                let target = stored_entity(model, operation, &spec.on, "query")?;
                let filters = resolve_fields(operation, target, &spec.filters, "filters")?;
                let ordering = ordering(operation, target, spec)?;
                query::lower(
                    model,
                    capability.id.as_str(),
                    operation,
                    target,
                    &filters,
                    &ordering,
                    spec.semantics.limit.unwrap_or(query::DEFAULT_LIMIT),
                )?
            }
            OperationKind::Transition(spec) => {
                transition::lower(model, capability.id.as_str(), operation, spec)?
            }
            OperationKind::Event(_) => continue,
        };
        output
            .insert(lowered.0, lowered.1)
            .map_err(CompileError::new)?;
    }
    Ok(())
}

fn stored_entity<'a>(
    model: &'a AppModel,
    operation: &Operation,
    target: &jails_model::EntityId,
    kind: &str,
) -> Result<&'a Entity, CompileError> {
    let target = entity(model, target)?;
    if !target.active || !target.facets.contains(&Facet::Repository) {
        return Err(CompileError::new(format!(
            "canonical {kind} `{}` targets an entity without active storage\n       fix: target an active scaffold or remove the `db` capability",
            operation.label
        )));
    }
    Ok(target)
}

/// The `publishEvent` calls for one operation's `emit` list.
///
/// Shared because commands and transitions publish identically and the rule
/// -- every emitted event, an event of another entity refuses -- must not get
/// two implementations that can disagree. `emit` is repeatable in
/// `jdl-sol.md` §12.2 and §12.4; transitions kept only the first and commands
/// published none at all.
///
/// The arguments are read off `result`, the row the statement returned, so an
/// event payload always reports what the database actually stored rather than
/// what the caller asked for.
pub(super) fn publications(
    model: &AppModel,
    operation: &Operation,
    target: &Entity,
    emits: &[jails_model::OperationId],
    imports: &mut BTreeSet<String>,
) -> Result<Vec<String>, CompileError> {
    let staged = outbox::delivery(operation) == jails_model::Delivery::Outbox;
    if staged {
        // Checked before a byte is rendered: exactly one event, staged by a
        // minted `id`. The refusals name the declaration to change.
        outbox::relayed(model, operation)?;
        imports.insert(format!(
            "{}.Jdbc{}Outbox",
            model.project.package_for(Package::Jobs),
            operation.names.java_type
        ));
        imports.insert("org.springframework.transaction.annotation.Transactional".to_string());
    }
    let mut publications = Vec::new();
    for event_id in emits {
        let yielded = model.operations.get(event_id).ok_or_else(|| {
            CompileError::new(format!(
                "linked operation `{}` references missing event `{event_id}`",
                operation.label
            ))
        })?;
        let OperationKind::Event(event) = &yielded.kind else {
            return Err(CompileError::new(format!(
                "linked operation `{}` emits non-event operation `{}`",
                operation.label, yielded.label
            )));
        };
        if event.on.as_ref().is_some_and(|entity| entity != &target.id) {
            return Err(CompileError::new(format!(
                "canonical operation `{}` emits event `{}` from another entity\n       fix: emit an event projected from `{}`",
                operation.label, yielded.label, target.label
            )));
        }
        let event_type = crate::emit_java::with_suffix(&yielded.names.java_type, "Event");
        imports.insert(format!(
            "{}.{event_type}",
            model.project.package_for(Package::DomainEvents)
        ));
        if !staged {
            imports.insert("org.springframework.context.ApplicationEventPublisher".to_string());
        }
        // **Read off `semantics.parameters`, not `fields`.** The flat list can
        // only name fields of the target entity; the linked parameters can
        // also carry a `Typed` component -- an event's own identity, a
        // timestamp -- and an emitter reading the flat form renders an empty
        // payload for an event declared with one. The linker folds `fields`
        // into the parameters, so this is the whole payload either way.
        let arguments = event
            .semantics
            .parameters
            .iter()
            .map(|parameter| match &parameter.source {
                jails_model::ParameterSource::Field(visible) => target
                    .field(&visible.field)
                    .map(|field| format!("result.{}()", field.names.java_member))
                    .ok_or_else(|| {
                        CompileError::new(format!(
                            "linked event `{}` references missing field `{}`",
                            yielded.label, visible.field
                        ))
                    }),
                // A component the target row does not carry needs a value
                // from somewhere, and a *direct* publication has none: the
                // command's own inputs are gone by the time the row comes
                // back, and inventing one is how an event's identity silently
                // became the row's. Staging happens inside the transaction
                // that made the row, which is where the two values a payload
                // legitimately mints -- its own id and the moment it happened
                // -- can be produced honestly.
                jails_model::ParameterSource::Typed(ty) if staged => match ty {
                    TypeRef::Builtin(BuiltinType::Uuid) => {
                        imports.insert(format!(
                            "{}.TimeOrderedUuid",
                            model.project.package_for(Package::Domain)
                        ));
                        Ok("TimeOrderedUuid.next()".to_string())
                    }
                    TypeRef::Builtin(BuiltinType::Instant) => {
                        imports.insert("java.time.Instant".to_string());
                        Ok("Instant.now()".to_string())
                    }
                    _ => Err(CompileError::new(format!(
                        "outbox event `{}` declares `{}`, which nothing in the staging transaction can supply\n       fix: project it from a field of `{}`, or declare it `uuid` (minted) or `instant` (now)",
                        yielded.label, parameter.name, target.label
                    ))),
                },
                jails_model::ParameterSource::Typed(_) => Err(CompileError::new(format!(
                    "canonical event `{}` declares `{}`, which the target row does not carry\n       fix: project it from a field of `{}`, or deliver this command through an outbox, which can mint one",
                    yielded.label, parameter.name, target.label
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        publications.push(if staged {
            format!("\n        outbox.stage(new {event_type}({arguments}));")
        } else {
            format!("\n        events.publishEvent(new {event_type}({arguments}));")
        });
    }
    Ok(publications)
}

/// The ordering this query renders, with its direction.
///
/// The direction is why this is not `resolve_fields`. It read a flat
/// `Vec<FieldId>` that had nowhere to hold one, so `order by [createdAt desc]`
/// compiled to `order by created_at` and a newest-first query returned
/// oldest-first with nothing to say so.
///
/// An ordering qualified by a join alias refuses: the `select` this emitter
/// builds names one table, so ordering by another one's column would produce
/// SQL that does not run.
fn ordering<'a>(
    operation: &Operation,
    target: &'a Entity,
    spec: &jails_model::Query,
) -> Result<Vec<(&'a Field, jails_model::SortDirection)>, CompileError> {
    spec.semantics
        .order
        .iter()
        .map(|ordering| {
            if ordering.field.qualifier.is_some() || ordering.field.entity != target.id {
                return Err(CompileError::new(format!(
                    "canonical query `{}` orders by a joined column
       fix: order by a field of `{}`, or eject this adapter and write the statement by hand",
                    operation.label, target.label
                )));
            }
            target
                .field(&ordering.field.field)
                .map(|field| (field, ordering.direction))
                .ok_or_else(|| {
                    CompileError::new(format!(
                        "linked operation `{}` references missing ordering field `{}`",
                        operation.label, ordering.field.field
                    ))
                })
        })
        .collect()
}

fn resolve_fields<'a>(
    operation: &Operation,
    target: &'a Entity,
    ids: &[jails_model::FieldId],
    role: &str,
) -> Result<Vec<&'a Field>, CompileError> {
    ids.iter()
        .map(|id| {
            target.field(id).ok_or_else(|| {
                CompileError::new(format!(
                    "linked operation `{}` references missing {role} field `{id}`",
                    operation.label
                ))
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn operation_file(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    type_name: &str,
    artifact_suffix: &str,
    compiler_pass: &str,
    imports: BTreeSet<String>,
    body: String,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let package = model.project.package_for(Package::AdaptersJdbc);
    let artifact_id = format!(
        "art_{capability_id}_{}_{artifact_suffix}",
        operation.id.as_str()
    );
    let rendered = render(&package, &imports, &body, &artifact_id);
    let path = ProjectPath::parse(format!(
        "{JAVA_ROOT}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))
    .map_err(CompileError::new)?;
    Ok((
        path,
        RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                    target.id.as_str().to_string(),
                ]),
                compiler_pass: compiler_pass.to_string(),
            },
        },
    ))
}

fn java_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn scopes(entity: &Entity) -> Vec<&Field> {
    entity
        .fields
        .iter()
        .filter(|field| field.semantics.scope.is_some())
        .collect()
}

fn context_parameter(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    if scopes(entity).is_empty() {
        String::new()
    } else {
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
        "ExecutionContext context, ".to_string()
    }
}

fn context_value(field: &Field, imports: &mut BTreeSet<String>) -> String {
    let scope = field
        .semantics
        .scope
        .as_ref()
        .expect("context values are requested only for scope fields");
    let claim = java_string(&scope.claim);
    match field.ty {
        TypeRef::Builtin(BuiltinType::String) => format!("context.claim({claim})"),
        TypeRef::Builtin(BuiltinType::Uuid) => {
            imports.insert("java.util.UUID".to_string());
            format!("UUID.fromString(context.claim({claim}))")
        }
        TypeRef::Builtin(BuiltinType::Integer) => {
            format!("Integer.parseInt(context.claim({claim}))")
        }
        TypeRef::Builtin(BuiltinType::Long) => {
            format!("Long.parseLong(context.claim({claim}))")
        }
        _ => unreachable!("the linker accepts only string, uuid, int, and long scope fields"),
    }
}

fn assignment_sql_value(
    operation: &Operation,
    field: &Field,
    value: &Value,
) -> Result<String, CompileError> {
    match value {
        Value::String(value) | Value::EnumConstant(value) => {
            Ok(format!("'{}'", value.replace('\'', "''")))
        }
        Value::Integer(value) | Value::Decimal(value) if safe_numeric_literal(value) => {
            Ok(value.clone())
        }
        Value::Boolean(value) => Ok(value.to_string()),
        Value::Function { name, arguments } if name == "now" && arguments.is_empty() => {
            Ok("current_timestamp".to_string())
        }
        _ => Err(CompileError::new(format!(
            "canonical operation `{}` cannot lower the constant assigned to `{}`\n       fix: use a string, enum, numeric, boolean, or `now()` constant, or eject the implementation boundary",
            operation.label, field.label
        ))),
    }
}

fn safe_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let mut has_digit = index > integer_start;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        has_digit |= index > fraction_start;
    }
    if !has_digit {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == exponent_start {
            return false;
        }
    }
    index == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::safe_numeric_literal;

    #[test]
    fn assignment_numbers_are_single_sql_literals_not_token_sequences() {
        for valid in ["0", "-12", "+3.5", ".25", "6e-4"] {
            assert!(safe_numeric_literal(valid), "rejected `{valid}`");
        }
        for invalid in ["", "+", ".", "1-2", "1e", "0;drop table task"] {
            assert!(!safe_numeric_literal(invalid), "accepted `{invalid}`");
        }
    }
}
