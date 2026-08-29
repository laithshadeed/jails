//! `add http` and `add format`: a server without a framework, and a build
//! that formats itself.
//!
//! `format` is best-effort by design -- it runs `spotless:apply` once so a
//! freshly generated project passes `jails check`, and a machine without a
//! usable Maven just gets a note.

use super::*;
use jails_protocol::feature::BuildFeature;

// ---------------------------------------------------------------------------
// http
// ---------------------------------------------------------------------------

/// An HTTP server with no dependency at all: `com.sun.net.httpserver` has
/// shipped in the JDK since 6 and is a supported API, and `java.net.http`
/// gives the test its client. A framework here would be the biggest dependency
/// in the project and buy nothing a route map does not.
pub(super) fn http_plan(slice: &Slice, name: Option<&str>) -> Result<Change> {
    let root: &Path = slice.root();
    let pkg: &str = &slice.placed(Layer::Api);
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Server");

    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: http_server_java(pkg, &class),
            },
            Artifact {
                kind: "capability file",
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: http_server_test_java(pkg, &class),
            },
        ],
        ..Change::default()
    })
}

pub(super) fn http_server_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("add/http_server_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn http_server_test_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("add/http_server_test_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

// ---------------------------------------------------------------------------
// format
// ---------------------------------------------------------------------------

/// Spotless, bound to `verify` as a check and available as `jails fmt` to
/// apply. Formatting nobody has to think about is the only kind that survives.
pub(super) fn format_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    Ok(Change {
        plugins: vec![(BuildFeature::Formatting, SPOTLESS_PLUGIN.to_string())],
        files: vec![Artifact {
            kind: "capability file",
            path: root.join(".editorconfig"),
            contents: EDITORCONFIG.to_string(),
        }],
        ..Change::default()
    })
}

const EDITORCONFIG: &str = r#"root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true

[*.{java,rs,xml}]
indent_style = space
indent_size = 4

[*.{yml,yaml,json,toml}]
indent_style = space
indent_size = 2

[Makefile]
indent_style = tab
"#;

/// palantir-java-format over google-java-format: it keeps a 120-column line,
/// which the generated code (records with several components, fluent AssertJ
/// chains) reads far better at than 100. Both are pinned -- a formatter that
/// drifts version rewrites files nobody touched.
pub(super) const SPOTLESS_PLUGIN: &str = r#"<plugin>
    <groupId>com.diffplug.spotless</groupId>
    <artifactId>spotless-maven-plugin</artifactId>
    <version>3.9.0</version>
    <configuration>
        <java>
            <palantirJavaFormat>
                <version>2.97.0</version>
            </palantirJavaFormat>
            <removeUnusedImports/>
        </java>
    </configuration>
    <executions>
        <execution>
            <id>spotless-check</id>
            <phase>verify</phase>
            <goals>
                <goal>check</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

// ---------------------------------------------------------------------------
// coverage
// ---------------------------------------------------------------------------

/// Coverage is a gate, not just a report someone may remember to inspect.
/// The threshold is intentionally explicit and can be raised in the POM as
/// the project matures; generated projects start with a useful 80% line bar.
pub(super) fn coverage_plan() -> Result<Change> {
    Ok(Change {
        plugins: vec![(BuildFeature::Coverage, JACOCO_PLUGIN.to_string())],
        ..Change::default()
    })
}

pub(super) const JACOCO_PLUGIN: &str = r#"<plugin>
    <groupId>org.jacoco</groupId>
    <artifactId>jacoco-maven-plugin</artifactId>
    <version>0.8.15</version>
    <executions>
        <execution>
            <id>coverage-agent</id>
            <goals>
                <goal>prepare-agent</goal>
            </goals>
        </execution>
        <execution>
            <id>coverage-report-and-check</id>
            <phase>verify</phase>
            <goals>
                <goal>report</goal>
                <goal>check</goal>
            </goals>
            <configuration>
                <rules>
                    <rule>
                        <element>BUNDLE</element>
                        <limits>
                            <limit>
                                <counter>LINE</counter>
                                <value>COVEREDRATIO</value>
                                <minimum>0.80</minimum>
                            </limit>
                        </limits>
                    </rule>
                </rules>
            </configuration>
        </execution>
    </executions>
</plugin>"#;

// ---------------------------------------------------------------------------
// loadtest
// ---------------------------------------------------------------------------

pub(super) fn loadtest_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let routes = crate::inspect::collect_routes(root);
    if routes.is_empty() {
        return Err(jails_support::Failure::Told(
            "no HTTP routes were found under src/main/java.\n       \
             fix: generate a controller, scaffold, or handler before `jails add loadtest`."
                .to_string(),
        ));
    }
    let dir = root.join("load-tests");
    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: dir.join("load-test.js"),
                contents: load_test_js(),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("api.js"),
                contents: load_api_js(&routes),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("payload-builder.js"),
                contents: payload_builder_js(),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("token-cache.js"),
                contents: token_cache_js(),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("Makefile"),
                contents: loadtest_makefile(),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("README.md"),
                contents: loadtest_readme(),
            },
        ],
        ..Change::default()
    })
}

fn load_test_js() -> String {
    r#"import { check, sleep } from 'k6';
import { request, routes } from './api.js';

export const options = {
  vus: Number(__ENV.VUS || 10),
  duration: __ENV.DURATION || '30s',
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500', 'p(99)<1000'],
  },
};

export default function () {
  const route = routes[__ITER % routes.length];
  const response = request(route);
  check(response, { 'status is below 500': (r) => r.status < 500 });
  sleep(0.1);
}
"#
    .to_string()
}

fn load_api_js(routes: &[crate::inspect::Route]) -> String {
    let entries = routes
        .iter()
        .map(|route| {
            format!(
                "  {{ method: {}, path: {}, handler: {} }}",
                crate::json::string(&route.verb),
                crate::json::string(&load_path(&route.path)),
                crate::json::string(&route.handler),
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"import http from 'k6/http';
import {{ payloadFor }} from './payload-builder.js';
import {{ authorizationHeaders }} from './token-cache.js';

const baseUrl = __ENV.BASE_URL || 'http://localhost:8080';

export const routes = [
{entries}
];

export function request(route) {{
  const params = {{ headers: {{ ...authorizationHeaders() }} }};
  if (['POST', 'PUT', 'PATCH'].includes(route.method)) {{
    params.headers['Content-Type'] = 'application/json';
    return http.request(route.method, `${{baseUrl}}${{route.path}}`, JSON.stringify(payloadFor(route)), params);
  }}
  return http.request(route.method, `${{baseUrl}}${{route.path}}`, null, params);
}}
"#
    )
}

fn load_path(path: &str) -> String {
    path.replace("[/{id}]", "/1").replace("{id}", "1")
}

fn payload_builder_js() -> String {
    r#"// Add representative route-specific bodies here. The fallback is valid JSON,
// so adding a generated route never breaks the load-test runner itself.
export function payloadFor(route) {
  return { route: route.handler, value: 'sample' };
}
"#
    .to_string()
}

fn token_cache_js() -> String {
    r#"let token;

export function authorizationHeaders() {
  token = token || __ENV.AUTH_TOKEN;
  return token ? { Authorization: `Bearer ${token}` } : {};
}
"#
    .to_string()
}

fn loadtest_makefile() -> String {
    "BASE_URL ?= http://localhost:8080\nVUS ?= 10\nDURATION ?= 30s\n\n.PHONY: run\nrun:\n\tk6 run -e BASE_URL=$(BASE_URL) -e VUS=$(VUS) -e DURATION=$(DURATION) load-test.js\n"
        .to_string()
}

fn loadtest_readme() -> String {
    r#"# Load tests

The route list in `api.js` was derived from the application's Java source by
`jails add loadtest`. Start the application, install [k6](https://k6.io/), and
run `make run`. Override `BASE_URL`, `VUS`, `DURATION`, or `AUTH_TOKEN` through
the environment. Re-run `jails remove loadtest && jails add loadtest` after
changing routes, after reviewing any local edits reported by `remove`.
"#
    .to_string()
}

// ---------------------------------------------------------------------------
// ci + docker
// ---------------------------------------------------------------------------

const CHECKOUT_SHA: &str = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"; // v6.0.2
const SETUP_JAVA_SHA: &str = "03ad4de0992f5dab5e18fcb136590ce7c4a0ac95"; // v5.6.0

pub(super) fn ci_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let release = project_release(slice.project())?;
    Ok(Change {
        files: vec![Artifact {
            kind: "capability file",
            path: root.join(".github/workflows/ci.yml"),
            contents: ci_workflow(release, root.join("mvnw").is_file()),
        }],
        ..Change::default()
    })
}

pub(super) fn docker_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let release = project_release(slice.project())?;
    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: root.join("Dockerfile"),
                contents: dockerfile(release, root.join("mvnw").is_file()),
            },
            Artifact {
                kind: "capability file",
                path: root.join(".dockerignore"),
                contents: dockerignore().to_string(),
            },
            Artifact {
                kind: "capability file",
                path: root.join(".github/workflows/image.yml"),
                contents: image_workflow(),
            },
        ],
        ..Change::default()
    })
}

pub(super) fn k8s_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let flavor: Flavor = slice.flavor();
    crate::spring::require_spring(flavor, "k8s")?;
    let pom = crate::pom::read(root)?;
    for (needle, fix) in [
        ("spring-boot-starter-actuator", "jails add actuator"),
        ("micrometer-registry-prometheus", "jails add observability"),
    ] {
        if !pom.contains(needle) {
            return Err(format!(
                "k8s probes and burn-rate alerts need `{needle}`.\n       fix: run `{fix}` first."
            )
            .into());
        }
    }
    if !root.join("Dockerfile").is_file() {
        return Err(jails_support::Failure::Told(
            "k8s needs the production image contract before it can deploy it.\n       \
             fix: run `jails add docker` first."
                .to_string(),
        ));
    }

    let raw_name = crate::project::artifact_id(&pom)
        .or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "application".to_string());
    let name = helm_name(&raw_name);
    let chart = root.join("deploy/chart");
    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: chart.join("Chart.yaml"),
                contents: chart_yaml(&name),
            },
            Artifact {
                kind: "capability file",
                path: chart.join("values.yaml"),
                contents: values_yaml(&name),
            },
            Artifact {
                kind: "capability file",
                path: chart.join("templates/deployment.yaml"),
                contents: deployment_yaml(&name),
            },
            Artifact {
                kind: "capability file",
                path: chart.join("templates/service.yaml"),
                contents: service_yaml(&name),
            },
            Artifact {
                kind: "capability file",
                path: chart.join("templates/configmap.yaml"),
                contents: configmap_yaml(&name),
            },
            Artifact {
                kind: "capability file",
                path: chart.join("templates/prometheus-rule.yaml"),
                contents: prometheus_rule_yaml(&name),
            },
        ],
        properties: vec![
            "# Kubernetes supplies POD_NAME from metadata.name; tag every replica separately."
                .to_string(),
            "management.metrics.tags.pod.name=${POD_NAME:unknown}".to_string(),
        ],
        ..Change::default()
    })
}

fn helm_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
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

/// The Helm chart, from the six templates the canonical compiler also reads.
///
/// One owner each, for the reason the CI workflow has one: a chart that
/// disagrees with itself between engines is a deployment that works from one
/// and not the other, and nothing about the difference is visible in a diff.
fn chart_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_chart.yaml").replace("{{NAME}}", name)
}

fn values_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_values.yaml").replace("{{NAME}}", name)
}

fn deployment_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_deployment.yaml").replace("{{NAME}}", name)
}

fn service_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_service.yaml").replace("{{NAME}}", name)
}

fn configmap_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_configmap.yaml").replace("{{NAME}}", name)
}

fn prometheus_rule_yaml(name: &str) -> String {
    include_str!("../../../../templates/add/k8s_prometheus_rule.yaml").replace("{{NAME}}", name)
}

/// The Java release, from the `Project` the caller already resolved.
///
/// It used to read the pom again. That is `abstract.md` §2's Feature Envy in
/// its smallest form: two answers to one question in one run, and the second
/// one arrives after `add` may have spliced the pom.
fn project_release(project: &crate::model::Project) -> Result<u32> {
    Ok(project.java_release().ok_or_else(|| {
        "pom.xml has no Java release; Jails cannot choose a compatible CI or container toolchain"
            .to_string()
    })?)
}

/// The verify workflow, rendered from the one template both engines read.
///
/// `templates/add/ci_workflow.yml` is shared with the canonical compiler
/// (`emit_capability/project_file.rs`). Two copies of a CI file would drift on
/// the pinned action SHAs, which is the drift nobody notices until a supply
/// chain advisory names a version this project still runs.
///
/// Substituted with `str::replace` rather than `template!`, deliberately:
/// GitHub spells its own expressions `${{ github.ref }}`, so a renderer that
/// treats `{{...}}` as a placeholder would read the workflow's own syntax as
/// keys. The tokens here are uppercase for the same reason -- they cannot
/// collide with anything GitHub writes.
pub(super) fn ci_workflow(release: u32, wrapper: bool) -> String {
    render_ci_workflow(release, if wrapper { "./mvnw" } else { "mvn" })
}

fn render_ci_workflow(release: u32, maven: &str) -> String {
    include_str!("../../../../templates/add/ci_workflow.yml")
        .replace("{{CHECKOUT_SHA}}", CHECKOUT_SHA)
        .replace("{{SETUP_JAVA_SHA}}", SETUP_JAVA_SHA)
        .replace("{{RELEASE}}", &release.to_string())
        .replace("{{MAVEN}}", maven)
}

/// The production image, rendered from the templates both engines read.
///
/// Shared with the canonical compiler for the reason `ci_workflow` is: two
/// copies of a container build drift on base image tags and on the pinned
/// action SHA, and neither drift is visible where anyone looks.
///
/// The build stage is chosen in Rust rather than by a template condition,
/// because which of the two it is depends on whether the project ships a
/// wrapper -- a structural choice, not a substitution, which is the line
/// `template.rs` draws.
pub(super) fn dockerfile(release: u32, wrapper: bool) -> String {
    let build = if wrapper {
        include_str!("../../../../templates/add/dockerfile_build_wrapper")
    } else {
        include_str!("../../../../templates/add/dockerfile_build_maven")
    }
    .replace("{{RELEASE}}", &release.to_string());
    include_str!("../../../../templates/add/dockerfile")
        .replace("{{BUILD_STAGE}}", &build)
        .replace("{{RELEASE}}", &release.to_string())
}

pub(super) fn dockerignore() -> &'static str {
    include_str!("../../../../templates/add/dockerignore")
}

/// The image workflow. Note the Go template inside it -- `{{.Config.User}}`,
/// which `docker image inspect` reads -- is why this is `str::replace` and not
/// a template engine: only the tokens named here are substituted, and every
/// other pair of braces in the file belongs to whoever will run it.
pub(super) fn image_workflow() -> String {
    include_str!("../../../../templates/add/image_workflow.yml")
        .replace("{{CHECKOUT_SHA}}", CHECKOUT_SHA)
}

#[cfg(test)]
mod delivery_tests {
    use super::*;

    #[test]
    fn ci_is_least_privilege_reproducible_and_runs_the_full_gate() {
        let source = ci_workflow(25, true);
        assert!(
            source.contains("permissions:\n  contents: read"),
            "{source}"
        );
        assert!(source.contains(CHECKOUT_SHA), "{source}");
        assert!(source.contains(SETUP_JAVA_SHA), "{source}");
        assert!(source.contains("./mvnw -B -ntp clean verify"), "{source}");
        assert!(source.contains("timeout-minutes: 30"), "{source}");
    }

    #[test]
    fn image_is_multi_stage_and_runs_as_a_numeric_non_root_user() {
        let source = dockerfile(25, true);
        assert!(source.contains("FROM eclipse-temurin:25-jdk-noble AS build"));
        assert!(source.contains("FROM eclipse-temurin:25-jre-noble"));
        assert!(source.contains("USER 10001:10001"));
        assert_eq!(source.matches("./mvnw -B -ntp").count(), 1, "{source}");
        assert!(source.contains("id=jails-maven-repository"), "{source}");
        assert!(!source.contains("dependency:go-offline"), "{source}");
        assert!(!source.contains("mvn "), "wrapper only: {source}");
    }

    #[test]
    fn existing_projects_without_a_wrapper_get_a_pinned_maven_builder() {
        let source = dockerfile(25, false);
        assert!(
            source.contains("FROM maven:3.9.16-eclipse-temurin-25 AS build"),
            "{source}"
        );
        assert!(source.contains("    mvn -B -ntp"), "{source}");
        assert_eq!(source.matches("mvn -B -ntp").count(), 1, "{source}");
        assert!(source.contains("id=jails-maven-repository"), "{source}");
        assert!(!source.contains("dependency:go-offline"), "{source}");
        assert!(ci_workflow(25, false).contains("run: mvn -B -ntp clean verify"));
    }
}
