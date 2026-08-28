//! Spring HTTP adapters for routed semantic operations.

use crate::CompileError;
use crate::emit_java::{
    JAVA_ROOT, domain_import, entity, java_type, primary_key, render, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Operation, OperationKind, StableId};
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
    let (route, port_package, port_type, return_type, parameters, mut imports) = match &operation
        .kind
    {
        OperationKind::Command(command) => {
            let Some(route) = command.route.as_deref() else {
                return Ok(None);
            };
            let entity = entity(model, &command.on)?;
            (
                route,
                "application.commands",
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
                route,
                "application.queries",
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
                route,
                "application.transitions",
                with_suffix(&operation.names.java_type, "Transition"),
                entity.names.java_type.clone(),
                format!("@PathVariable(\"id\") {key_type} id, @RequestBody PORT.Input input"),
                imports,
            )
        }
        OperationKind::Event(_) => return Ok(None),
    };
    let (method, path) = split_route(route)?;
    let package = format!("{}.adapters.http", model.project.base_package);
    let type_name = with_suffix(&operation.names.java_type, "Controller");
    imports.extend([
        format!(
            "{}.{}.{}",
            model.project.base_package, port_package, port_type
        ),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestMethod".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    let parameters = parameters.replace("PORT", &port_type);
    let invocation = if matches!(operation.kind, OperationKind::Transition(_)) {
        "operation.execute(id, input)"
    } else {
        "operation.execute(input)"
    };
    let body = format!(
        "@RestController\npublic final class {type_name} {{\n\n    private final {port_type} operation;\n\n    public {type_name}({port_type} operation) {{\n        this.operation = operation;\n    }}\n\n    @RequestMapping(path = \"{path}\", method = RequestMethod.{method})\n    public {return_type} execute({parameters}) {{\n        return {invocation};\n    }}\n}}"
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
