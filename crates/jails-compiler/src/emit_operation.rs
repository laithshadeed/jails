//! Executable JDBC adapters for canonical operations.

mod command;
mod query;
mod transition;

use crate::CompileError;
use crate::emit_java::{JAVA_ROOT, entity, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, Facet, Field, Operation, OperationKind, StableId, TypeRef,
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
