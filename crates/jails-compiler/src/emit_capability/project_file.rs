//! Whole project files emitted through the generic reader-file merge protocol.

use super::reader_facet;
use crate::CompileError;
use jails_contracts::{FileMode, ProjectPath, RenderedTree};
use jails_model::{AppModel, Capability, EndpointMethod, OperationKind, UnitKind};

const CI_WORKFLOW_PATH: &str = ".github/workflows/ci.yml";

const EDITORCONFIG_PATH: &str = ".editorconfig";

const DOCKER_PATHS: [&str; 3] = ["Dockerfile", ".dockerignore", ".github/workflows/image.yml"];

/// The Helm chart, as (suffix, path, template).
const K8S_FILES: [(&str, &str, crate::Template); 6] = [
    (
        "chart",
        "deploy/chart/Chart.yaml",
        crate::template!("add/k8s_chart.yaml"),
    ),
    (
        "values",
        "deploy/chart/values.yaml",
        crate::template!("add/k8s_values.yaml"),
    ),
    (
        "deployment",
        "deploy/chart/templates/deployment.yaml",
        crate::template!("add/k8s_deployment.yaml"),
    ),
    (
        "service",
        "deploy/chart/templates/service.yaml",
        crate::template!("add/k8s_service.yaml"),
    ),
    (
        "configmap",
        "deploy/chart/templates/configmap.yaml",
        crate::template!("add/k8s_configmap.yaml"),
    ),
    (
        "prometheus-rule",
        "deploy/chart/templates/prometheus-rule.yaml",
        crate::template!("add/k8s_prometheus_rule.yaml"),
    ),
];

/// Pinned by commit, not by tag: a tag is mutable and a moved tag is a supply
/// chain compromise nobody sees in the diff.
const CHECKOUT_SHA: &str = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"; // v6.0.2
const SETUP_JAVA_SHA: &str = "03ad4de0992f5dab5e18fcb136590ce7c4a0ac95"; // v5.6.0

const LOADTEST_PATHS: &[(&str, &str, crate::Template)] = &[
    (
        "runner",
        "load-tests/load-test.js",
        crate::template!("add/loadtest_load_test.js"),
    ),
    (
        "payload",
        "load-tests/payload-builder.js",
        crate::template!("add/loadtest_payload_builder.js"),
    ),
    (
        "token",
        "load-tests/token-cache.js",
        crate::template!("add/loadtest_token_cache.js"),
    ),
    (
        "makefile",
        "load-tests/Makefile",
        crate::template!("add/loadtest_makefile"),
    ),
    (
        "readme",
        "load-tests/README.md",
        crate::template!("add/loadtest_readme.md"),
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
    if has(model, "format") {
        paths
            .push(ProjectPath::parse(EDITORCONFIG_PATH).expect("registered project path is valid"));
    }
    if has(model, "k8s") {
        paths.extend(K8S_FILES.iter().map(|(_, path, _)| {
            ProjectPath::parse(*path).expect("registered project path is valid")
        }));
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
    project: &jails_contracts::ProjectFacts,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    match capability.kind.as_str() {
        "loadtest" => lower_loadtest(model, capability, output, templates),
        "ci" => lower_ci(model, capability, output, project.maven_wrapper, templates),
        "docker" => lower_docker(model, capability, output, project.maven_wrapper, templates),
        "k8s" => lower_k8s(model, capability, output, templates),
        "format" => lower_format(capability, output, templates),
        // **One keep file, whichever storage declared it.** Both storage
        // capabilities fill the same directory, and two facets targeting one
        // path is a collision the executor refuses -- correctly, since it
        // cannot know which owner's bytes win. `db` claims it when both are
        // present, because it is the one whose migrations Flyway runs.
        "db" => lower_migration_directory(capability, output),
        "sqlite" if !model.capabilities.values().any(|other| other.kind == "db") => {
            lower_migration_directory(capability, output)
        }
        _ => Ok(()),
    }
}

/// The migration directory, established by the capability that will fill it.
///
/// **A directory git does not track is a directory a colleague does not get.**
/// Forward migrations are the reader's own history -- they land in
/// `src/main/resources/db/migration` and are never rewritten -- so the
/// location has to exist before the first one is appended, and an empty
/// directory does not survive a clone. It is one keep file rather than
/// anything the schema depends on: Flyway is content with a location holding
/// nothing, and the first `g scaffold` writes the migration beside it.
fn lower_migration_directory(
    capability: &Capability,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    reader_facet::emit_managed_file(
        output,
        capability,
        "migration-directory",
        ProjectPath::parse("src/main/resources/db/migration/.gitkeep")
            .map_err(CompileError::new)?,
        Vec::new(),
        FileMode::Regular,
    )
}

/// The verify workflow.
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
    maven_wrapper: bool,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    let workflow = crate::template!("add/ci_workflow.yml")
        .resolve(templates)?
        .replace("{{CHECKOUT_SHA}}", CHECKOUT_SHA)
        .replace("{{SETUP_JAVA_SHA}}", SETUP_JAVA_SHA)
        .replace("{{RELEASE}}", &model.project.java_release.to_string())
        .replace("{{MAVEN}}", if maven_wrapper { "./mvnw" } else { "mvn" });
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
/// The build stage is chosen here rather than in a template, because which
/// one applies depends on whether the project ships a wrapper -- structural,
/// not substitution.
fn lower_docker(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
    maven_wrapper: bool,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    let release = model.project.java_release.to_string();
    let build = if maven_wrapper {
        crate::template!("add/dockerfile_build_wrapper")
    } else {
        crate::template!("add/dockerfile_build_maven")
    }
    .resolve(templates)?
    .replace("{{RELEASE}}", &release);
    let files = [
        (
            "dockerfile",
            DOCKER_PATHS[0],
            crate::template!("add/dockerfile")
                .resolve(templates)?
                .replace("{{BUILD_STAGE}}", &build)
                .replace("{{RELEASE}}", &release),
        ),
        (
            "dockerignore",
            DOCKER_PATHS[1],
            crate::template!("add/dockerignore")
                .resolve(templates)?
                .to_string(),
        ),
        (
            "image-workflow",
            DOCKER_PATHS[2],
            crate::template!("add/image_workflow.yml")
                .resolve(templates)?
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

/// `format`'s reader-facing half: the editor settings the formatter assumes.
///
/// Spotless enforces Java; `.editorconfig` is what stops an editor fighting it
/// in every other file.
fn lower_format(
    capability: &Capability,
    output: &mut RenderedTree,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    reader_facet::emit_managed_file(
        output,
        capability,
        "editorconfig",
        ProjectPath::parse(EDITORCONFIG_PATH).map_err(CompileError::new)?,
        crate::template!("add/editorconfig")
            .resolve(templates)?
            .as_bytes()
            .to_vec(),
        FileMode::Regular,
    )
}

/// The Helm chart, and the three declarations it cannot deploy without.
///
/// All three are model questions -- the capability is declared or it is not
/// -- which is both simpler and stricter than reading the pom: a project with
/// `spring-boot-starter-actuator` spliced in by hand has no actuator
/// capability for `sync` to reconcile.
///
/// The chart is named from the model's project name rather than the pom's
/// artifactId, because `AppModel` is where names come from; the two differ
/// for a project whose declared name and coordinate disagree.
fn lower_k8s(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    for (kind, fix) in [
        ("actuator", "jails add actuator"),
        ("observability", "jails add observability"),
        ("docker", "jails add docker"),
    ] {
        if !has(model, kind) {
            return Err(CompileError::new(format!(
                "k8s probes, burn-rate alerts and the image it deploys need the `{kind}` capability.\n       fix: run `{fix}` first."
            )));
        }
    }
    let name = helm_name(&model.project.name);
    for (suffix, path, template) in K8S_FILES {
        reader_facet::emit_managed_file(
            output,
            capability,
            suffix,
            ProjectPath::parse(path).map_err(CompileError::new)?,
            template
                .resolve(templates)?
                .replace("{{NAME}}", &name)
                .into_bytes(),
            FileMode::Regular,
        )?;
    }
    Ok(())
}

/// A DNS-1123 label: lowercase alphanumerics and single hyphens, 63 bytes.
///
/// Kubernetes rejects anything else, and it rejects it at apply time -- long
/// after the chart was generated and committed.
fn helm_name(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "application".to_string()
    } else {
        out.chars().take(63).collect()
    }
}

fn lower_loadtest(
    model: &AppModel,
    capability: &Capability,
    output: &mut RenderedTree,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    let routes = routes(model);
    if routes.is_empty() {
        // **Phrased as the state, not as the command**, because the compiler
        // is pure over (snapshot, patch) and cannot tell installing the
        // capability from removing the last route out from under one already
        // installed. Naming only the first would send the reader in the second
        // case to run a command they have run -- and it is the same refusal
        // every other reference the model protects gives.
        return Err(CompileError::new(
            "removing the last route would leave the `loadtest` capability pointing at nothing.\n       fix: keep a controller or routed operation, or run `jails remove loadtest`",
        ));
    }
    for (suffix, path, template) in LOADTEST_PATHS {
        reader_facet::emit_managed_file(
            output,
            capability,
            suffix,
            ProjectPath::parse(*path).map_err(CompileError::new)?,
            template.resolve(templates)?.as_bytes().to_vec(),
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
    let api = crate::template!("add/loadtest_api.js")
        .resolve(templates)?
        .replace("{{ROUTES}}", &entries);
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
