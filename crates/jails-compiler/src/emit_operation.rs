//! Executable JDBC adapters for canonical operations.

mod command;
mod query;
mod transition;

use crate::CompileError;
use crate::emit_java::{JAVA_ROOT, entity, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, Facet, Field, Operation, OperationKind, StableId, TypeRef, Value,
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
                let ordering = resolve_fields(operation, target, &spec.order_by, "ordering")?;
                query::lower(
                    model,
                    capability.id.as_str(),
                    operation,
                    target,
                    &filters,
                    &ordering,
                    spec.limit.unwrap_or(query::DEFAULT_LIMIT),
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

fn resolve_fields<'a>(
    operation: &Operation,
    target: &'a Entity,
    ids: &[jails_model::FieldId],
    role: &str,
) -> Result<Vec<&'a Field>, CompileError> {
    ids.iter()
        .map(|id| {
            target.fields.get(id).ok_or_else(|| {
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
    let package = format!("{}.adapters.jdbc", model.project.base_package);
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
        .values()
        .filter(|field| field.semantics.scope.is_some())
        .collect()
}

fn context_parameter(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    if scopes(entity).is_empty() {
        String::new()
    } else {
        imports.insert(format!(
            "{}.application.ExecutionContext",
            model.project.base_package
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
