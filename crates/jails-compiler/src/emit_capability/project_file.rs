//! Whole project files emitted through the generic reader-file merge protocol.

use super::reader_facet;
use crate::CompileError;
use jails_contracts::{FileMode, ProjectPath, RenderedTree};
use jails_model::{AppModel, Capability, EndpointMethod, OperationKind, UnitKind};

const CI_WORKFLOW_PATH: &str = ".github/workflows/ci.yml";

const DOCKER_PATHS: [&str; 3] = ["Dockerfile", ".dockerignore", ".github/workflows/image.yml"];

/// Pinned by commit, not by tag: a tag is mutable and a moved tag is a supply
/// chain compromise nobody sees in the diff. Kept in step with
/// `add/tooling.rs`, which pins the same two.
const CHECKOUT_SHA: &str = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"; // v6.0.2
const SETUP_JAVA_SHA: &str = "03ad4de0992f5dab5e18fcb136590ce7c4a0ac95"; // v5.6.0

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
    let mut paths = Vec::new();
    if has(model, "ci") {
        paths.push(ProjectPath::parse(CI_WORKFLOW_PATH).expect("registered project path is valid"));
    }
    if has(model, "docker") {
        paths.extend(
            DOCKER_PATHS
                .iter()
                .map(|path| ProjectPath::parse(*path).expect("registered project path is valid")),
        );
    }
    paths.extend(loadtest_paths(model));
    paths
}

fn loadtest_paths(model: &AppModel) -> Vec<ProjectPath> {
    if !has(model, "loadtest") {
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
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    match capability.kind.as_str() {
        "loadtest" => lower_loadtest(model, capability, output),
        "ci" => lower_ci(model, capability, output, observed),
        "docker" => lower_docker(model, capability, output, observed),
        _ => Ok(()),
    }
}

/// The verify workflow, from the template `add/tooling.rs` also reads.
///
/// One template, because two copies of a CI file drift on the pinned action
/// SHAs -- and that is the drift nobody notices until an advisory names a
/// version this project still runs.
///
/// `maven_wrapper` is the reason this capability needs an observed fact at
/// all: `./mvnw` on a project without a wrapper fails at the first step, and
/// `mvn` on a project with one silently uses whatever Maven the runner has,
/// which is the version drift the wrapper exists to prevent.
fn lower_ci(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    let workflow = include_str!("../../../../templates/add/ci_workflow.yml")
        .replace("{{CHECKOUT_SHA}}", CHECKOUT_SHA)
        .replace("{{SETUP_JAVA_SHA}}", SETUP_JAVA_SHA)
        .replace("{{RELEASE}}", &model.project.java_release.to_string())
        .replace(
            "{{MAVEN}}",
            if observed.maven_wrapper {
                "./mvnw"
            } else {
                "mvn"
            },
        );
    reader_facet::emit_managed_file(
        output,
        capability,
        "workflow",
        ProjectPath::parse(CI_WORKFLOW_PATH).map_err(CompileError::new)?,
        workflow.into_bytes(),
        FileMode::Regular,
    )
}

/// The production image: a Dockerfile, its ignore file, and the workflow that
/// proves the image runs as a numeric non-root user.
///
/// Same three templates the legacy engine reads. The build stage is chosen
/// here rather than in a template, because which one applies depends on
/// whether the project ships a wrapper -- structural, not substitution.
fn lower_docker(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    let release = model.project.java_release.to_string();
    let build = if observed.maven_wrapper {
        include_str!("../../../../templates/add/dockerfile_build_wrapper")
    } else {
        include_str!("../../../../templates/add/dockerfile_build_maven")
    }
    .replace("{{RELEASE}}", &release);
    let files = [
        (
            "dockerfile",
            DOCKER_PATHS[0],
            include_str!("../../../../templates/add/dockerfile")
                .replace("{{BUILD_STAGE}}", &build)
                .replace("{{RELEASE}}", &release),
        ),
        (
            "dockerignore",
            DOCKER_PATHS[1],
            include_str!("../../../../templates/add/dockerignore").to_string(),
        ),
        (
            "image-workflow",
            DOCKER_PATHS[2],
            include_str!("../../../../templates/add/image_workflow.yml")
                .replace("{{CHECKOUT_SHA}}", CHECKOUT_SHA),
        ),
    ];
    for (suffix, path, body) in files {
        reader_facet::emit_managed_file(
            output,
            capability,
            suffix,
            ProjectPath::parse(path).map_err(CompileError::new)?,
            body.into_bytes(),
            FileMode::Regular,
        )?;
    }
    Ok(())
}

fn lower_loadtest(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
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

fn has(model: &AppModel, kind: &str) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == kind)
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
