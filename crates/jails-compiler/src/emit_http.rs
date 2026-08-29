//! Spring HTTP adapters for routed semantic operations.

use crate::CompileError;
use crate::emit_java::{
    JAVA_ROOT, domain_import, entity, java_type, primary_key, render, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Operation, OperationKind, Package, StableId};
use std::collections::BTreeSet;

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "api")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        let Some((path, file)) = lower(model, capability.id.as_str(), operation)? else {
            continue;
        };
        output.insert(path, file).map_err(CompileError::new)?;
    }
    Ok(())
}

fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let (target, route, port_package, port_type, return_type, parameters, mut imports) =
        match &operation.kind {
            OperationKind::Command(command) => {
                let Some(route) = command.route.as_deref() else {
                    return Ok(None);
                };
                let entity = entity(model, &command.on)?;
                (
                    entity,
                    route,
                    Package::ApplicationCommands,
                    with_suffix(&operation.names.java_type, "Command"),
                    entity.names.java_type.clone(),
                    "@RequestBody PORT.Input input".to_string(),
                    BTreeSet::from([
                        domain_import(model, entity),
                        "org.springframework.web.bind.annotation.RequestBody".to_string(),
                    ]),
                )
            }
            OperationKind::Query(query) => {
                let Some(route) = query.route.as_deref() else {
                    return Ok(None);
                };
                let entity = entity(model, &query.on)?;
                (
                    entity,
                    route,
                    Package::ApplicationQueries,
                    with_suffix(&operation.names.java_type, "Query"),
                    format!("List<{}>", entity.names.java_type),
                    "@ModelAttribute PORT.Input input".to_string(),
                    BTreeSet::from([
                        domain_import(model, entity),
                        "java.util.List".to_string(),
                        "org.springframework.web.bind.annotation.ModelAttribute".to_string(),
                    ]),
                )
            }
            OperationKind::Transition(transition) => {
                let Some(route) = transition.route.as_deref() else {
                    return Ok(None);
                };
                let (_, path) = split_route(route)?;
                if !path.contains("{id}") {
                    return Err(CompileError::new(format!(
                        "transition operation `{}` needs `{{id}}` in its API route\n       fix: set `route = \"PATCH /path/{{id}}\"` or remove the `api` capability",
                        operation.label
                    )));
                }
                let entity = entity(model, &transition.on)?;
                let primary_key = primary_key(entity)?;
                let mut imports = BTreeSet::from([
                    domain_import(model, entity),
                    "org.springframework.web.bind.annotation.PathVariable".to_string(),
                    "org.springframework.web.bind.annotation.RequestBody".to_string(),
                ]);
                let key_type = java_type(primary_key, &mut imports);
                (
                    entity,
                    route,
                    Package::ApplicationTransitions,
                    with_suffix(&operation.names.java_type, "Transition"),
                    entity.names.java_type.clone(),
                    format!("@PathVariable(\"id\") {key_type} id, @RequestBody PORT.Input input"),
                    imports,
                )
            }
            OperationKind::Event(_) => return Ok(None),
        };
    let (method, path) = split_route(route)?;
    let package = model.project.package_for(Package::AdaptersHttp);
    let type_name = with_suffix(&operation.names.java_type, "Controller");
    imports.extend([
        format!("{}.{port_type}", model.project.package_for(port_package)),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestMethod".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    let mut parameters = parameters.replace("PORT", &port_type);
    let scope_fields = target
        .fields
        .iter()
        .filter(|field| field.semantics.scope.is_some())
        .collect::<Vec<_>>();
    let (scope_member, scope_parameter, scope_assignment, context_setup, context_argument) =
        if scope_fields.is_empty() {
            (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            )
        } else {
            imports.extend([
                format!(
                    "{}.ScopeAuthorizer",
                    model.project.package_for(Package::Base)
                ),
                format!(
                    "{}.ExecutionContext",
                    model.project.package_for(Package::Application)
                ),
                "java.util.Map".to_string(),
                "org.springframework.security.core.Authentication".to_string(),
            ]);
            parameters.push_str(", Authentication authentication");
            let entries = scope_fields
                .iter()
                .map(|field| {
                    let claim = java_string(
                        &field
                            .semantics
                            .scope
                            .as_ref()
                            .expect("selected scope field")
                            .claim,
                    );
                    format!(
                        "                Map.entry({claim}, scopes.claim(authentication, {claim}))"
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            (
                "\n    private final ScopeAuthorizer scopes;".to_string(),
                ", ScopeAuthorizer scopes".to_string(),
                "\n        this.scopes = scopes;".to_string(),
                format!(
                    "        var context = new ExecutionContext(Map.ofEntries(\n{entries}));\n"
                ),
                "context, ".to_string(),
            )
        };
    let invocation = if matches!(operation.kind, OperationKind::Transition(_)) {
        format!("operation.execute({context_argument}id, input)")
    } else {
        format!("operation.execute({context_argument}input)")
    };
    let body = format!(
        "@RestController\npublic final class {type_name} {{\n\n    private final {port_type} operation;{scope_member}\n\n    public {type_name}({port_type} operation{scope_parameter}) {{\n        this.operation = operation;{scope_assignment}\n    }}\n\n    @RequestMapping(path = \"{path}\", method = RequestMethod.{method})\n    public {return_type} execute({parameters}) {{\n{context_setup}        return {invocation};\n    }}\n}}"
    );
    let artifact_id = format!("art_{}_http", operation.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some((
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
                ]),
                compiler_pass: "capability-api".to_string(),
            },
        },
    )))
}

fn split_route(route: &str) -> Result<(&str, &str), CompileError> {
    route.split_once(' ').ok_or_else(|| {
        CompileError::new(format!(
            "linked operation contains invalid route `{route}`\n       fix: use `METHOD /path`"
        ))
    })
}

fn java_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
