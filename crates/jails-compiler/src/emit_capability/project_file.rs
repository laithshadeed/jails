//! Whole project files emitted through the generic reader-file merge protocol.

use super::reader_facet;
use crate::CompileError;
use jails_contracts::{FileMode, ProjectPath, RenderedTree};
use jails_model::{AppModel, Capability, EndpointMethod, OperationKind, UnitKind};

const LOADTEST_PATHS: &[(&str, &str, &str)] = &[
    (
        "runner",
        "load-tests/load-test.js",
        include_str!("../../../../templates/add/loadtest_load_test.js"),
    ),
    (
        "payload",
        "load-tests/payload-builder.js",
        include_str!("../../../../templates/add/loadtest_payload_builder.js"),
    ),
    (
        "token",
        "load-tests/token-cache.js",
        include_str!("../../../../templates/add/loadtest_token_cache.js"),
    ),
    (
        "makefile",
        "load-tests/Makefile",
        include_str!("../../../../templates/add/loadtest_makefile"),
    ),
    (
        "readme",
        "load-tests/README.md",
        include_str!("../../../../templates/add/loadtest_readme.md"),
    ),
];

struct Route {
    method: String,
    path: String,
    handler: String,
}

pub(super) fn paths(model: &AppModel) -> Vec<ProjectPath> {
    if !has_loadtest(model) {
        return Vec::new();
    }
    LOADTEST_PATHS
        .iter()
        .map(|(_, path, _)| *path)
        .chain(["load-tests/api.js"])
        .map(|path| ProjectPath::parse(path).expect("registered project path is valid"))
        .collect()
}

pub(super) fn lower_and_emit(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    if capability.kind != "loadtest" {
        return Ok(());
    }
    let routes = routes(model);
    if routes.is_empty() {
        return Err(CompileError::new(
            "no HTTP routes are declared in the canonical model.\n       fix: generate a controller or routed operation before `jails add loadtest`.",
        ));
    }
    for (suffix, path, template) in LOADTEST_PATHS {
        reader_facet::emit_managed_file(
            output,
            capability,
            suffix,
            ProjectPath::parse(*path).map_err(CompileError::new)?,
            template.as_bytes().to_vec(),
            FileMode::Regular,
        )?;
    }
    let entries = routes
        .iter()
        .map(|route| {
            format!(
                "  {{ method: {}, path: {}, handler: {} }}",
                serde_json::to_string(&route.method).expect("route method is serializable"),
                serde_json::to_string(&load_path(&route.path)).expect("route path is serializable"),
                serde_json::to_string(&route.handler).expect("route handler is serializable"),
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let api =
        include_str!("../../../../templates/add/loadtest_api.js").replace("{{ROUTES}}", &entries);
    reader_facet::emit_managed_file(
        output,
        capability,
        "api",
        ProjectPath::parse("load-tests/api.js").expect("registered project path is valid"),
        api.into_bytes(),
        FileMode::Regular,
    )
}

fn has_loadtest(model: &AppModel) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == "loadtest")
}

fn routes(model: &AppModel) -> Vec<Route> {
    let mut routes = model
        .units
        .values()
        .filter(|unit| unit.kind == UnitKind::Controller)
        .filter_map(|unit| {
            let endpoint = unit.endpoint.as_ref()?;
            Some(Route {
                method: method(endpoint.method).to_string(),
                path: endpoint.path.clone(),
                handler: format!("{}#{}", unit.java_type, handler(endpoint.method)),
            })
        })
        .collect::<Vec<_>>();
    let has_api = model
        .capabilities
        .values()
        .any(|capability| capability.kind == "api");
    if has_api {
        routes.extend(model.operations.values().filter_map(|operation| {
            let route = match &operation.kind {
                OperationKind::Command(command) => command.route.as_deref(),
                OperationKind::Query(query) => query.route.as_deref(),
                OperationKind::Transition(transition) => transition.route.as_deref(),
                OperationKind::Event(_) => None,
            }?;
            let (method, path) = route.split_once(' ')?;
            Some(Route {
                method: method.to_string(),
                path: path.to_string(),
                handler: format!("{}Controller#execute", operation.names.java_type),
            })
        }));
    }
    routes.sort_by(|left, right| {
        (&left.path, &left.method, &left.handler).cmp(&(&right.path, &right.method, &right.handler))
    });
    routes.dedup_by(|left, right| {
        left.path == right.path && left.method == right.method && left.handler == right.handler
    });
    routes
}

fn method(method: EndpointMethod) -> &'static str {
    match method {
        EndpointMethod::Get => "GET",
        EndpointMethod::Post => "POST",
        EndpointMethod::Put => "PUT",
        EndpointMethod::Patch => "PATCH",
        EndpointMethod::Delete => "DELETE",
    }
}

fn handler(method: EndpointMethod) -> &'static str {
    match method {
        EndpointMethod::Get => "get",
        EndpointMethod::Post => "post",
        EndpointMethod::Put => "put",
        EndpointMethod::Patch => "patch",
        EndpointMethod::Delete => "delete",
    }
}

fn load_path(path: &str) -> String {
    path.replace("[/{id}]", "/1").replace("{id}", "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_path_materializes_generated_identifiers() {
        assert_eq!(load_path("/tasks[/{id}]"), "/tasks/1");
        assert_eq!(load_path("/tasks/{id}"), "/tasks/1");
    }
}
