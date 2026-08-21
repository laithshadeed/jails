//! `add http` and `add format`: a server without a framework, and a build
//! that formats itself.
//!
//! `format` is best-effort by design -- it runs `spotless:apply` once so a
//! freshly generated project passes `jails check`, and a machine without a
//! usable Maven just gets a note.

use super::*;

// ---------------------------------------------------------------------------
// http
// ---------------------------------------------------------------------------

/// An HTTP server with no dependency at all: `com.sun.net.httpserver` has
/// shipped in the JDK since 6 and is a supported API, and `java.net.http`
/// gives the test its client. A framework here would be the biggest dependency
/// in the project and buy nothing a route map does not.
pub(super) fn http_plan(root: &std::path::Path, pkg: &str, name: Option<&str>) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Server");

    Ok(Plan {
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: http_server_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: http_server_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

pub(super) fn http_server_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/http_server_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn http_server_test_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/http_server_test_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

// ---------------------------------------------------------------------------
// format
// ---------------------------------------------------------------------------

/// Spotless, bound to `verify` as a check and available as `jails fmt` to
/// apply. Formatting nobody has to think about is the only kind that survives.
pub(super) const SPOTLESS_ARTIFACT: &str = "spotless-maven-plugin";

pub(super) fn format_plan() -> Result<Plan> {
    Ok(Plan {
        plugins: vec![(SPOTLESS_ARTIFACT, SPOTLESS_PLUGIN.to_string())],
        ..Plan::default()
    })
}

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
// ci + docker
// ---------------------------------------------------------------------------

const CHECKOUT_SHA: &str = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"; // v6.0.2
const SETUP_JAVA_SHA: &str = "03ad4de0992f5dab5e18fcb136590ce7c4a0ac95"; // v5.6.0

pub(super) fn ci_plan(root: &Path) -> Result<Plan> {
    let release = project_release(root)?;
    Ok(Plan {
        files: vec![NewFile {
            path: root.join(".github/workflows/ci.yml"),
            contents: ci_workflow(release, root.join("mvnw").is_file()),
        }],
        ..Plan::default()
    })
}

pub(super) fn docker_plan(root: &Path) -> Result<Plan> {
    let release = project_release(root)?;
    Ok(Plan {
        files: vec![
            NewFile {
                path: root.join("Dockerfile"),
                contents: dockerfile(release, root.join("mvnw").is_file()),
            },
            NewFile {
                path: root.join(".dockerignore"),
                contents: dockerignore().to_string(),
            },
            NewFile {
                path: root.join(".github/workflows/image.yml"),
                contents: image_workflow(),
            },
        ],
        ..Plan::default()
    })
}

fn project_release(root: &Path) -> Result<u32> {
    let text = crate::pom::read(root)?;
    crate::pom::release_level(&text).ok_or_else(|| {
        "pom.xml has no Java release; Jails cannot choose a compatible CI or container toolchain"
            .to_string()
    })
}

fn ci_workflow(release: u32, wrapper: bool) -> String {
    let maven = if wrapper { "./mvnw" } else { "mvn" };
    format!(
        r#"name: verify

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: verify-${{{{ github.workflow }}}}-${{{{ github.ref }}}}
  cancel-in-progress: true

jobs:
  verify:
    runs-on: ubuntu-24.04
    timeout-minutes: 30
    steps:
      - name: Check out source
        uses: actions/checkout@{CHECKOUT_SHA} # v6.0.2
        with:
          persist-credentials: false
      - name: Set up Java
        uses: actions/setup-java@{SETUP_JAVA_SHA} # v5.6.0
        with:
          distribution: temurin
          java-version: '{release}'
          cache: maven
      - name: Verify
        run: {maven} -B -ntp clean verify
"#
    )
}

fn dockerfile(release: u32, wrapper: bool) -> String {
    let build = if wrapper {
        format!(
            r#"FROM eclipse-temurin:{release}-jdk-noble AS build
WORKDIR /workspace
COPY .mvn/ .mvn/
COPY mvnw pom.xml ./
RUN ./mvnw -B -ntp -DskipTests dependency:go-offline
COPY src/ src/
RUN ./mvnw -B -ntp -DskipTests package \
    && cp "$(find target -maxdepth 1 -type f -name '*.jar' ! -name '*.original' -print -quit)" /workspace/application.jar
"#
        )
    } else {
        format!(
            r#"FROM maven:3.9.16-eclipse-temurin-{release} AS build
WORKDIR /workspace
COPY pom.xml ./
RUN mvn -B -ntp -DskipTests dependency:go-offline
COPY src/ src/
RUN mvn -B -ntp -DskipTests package \
    && cp "$(find target -maxdepth 1 -type f -name '*.jar' ! -name '*.original' -print -quit)" /workspace/application.jar
"#
        )
    };
    format!(
        r#"# syntax=docker/dockerfile:1
{build}

FROM eclipse-temurin:{release}-jre-noble
WORKDIR /app
COPY --from=build --chown=10001:10001 /workspace/application.jar /app/application.jar
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["java", "-XX:MaxRAMPercentage=75.0", "-Djava.io.tmpdir=/tmp", "-jar", "/app/application.jar"]
"#
    )
}

fn dockerignore() -> &'static str {
    r#".git
.github
.idea
.jails/app-state-v1
.vscode
target
*.iml
compose.yaml
"#
}

fn image_workflow() -> String {
    format!(
        r#"name: image

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: image-${{{{ github.workflow }}}}-${{{{ github.ref }}}}
  cancel-in-progress: true

jobs:
  image:
    runs-on: ubuntu-24.04
    timeout-minutes: 30
    steps:
      - name: Check out source
        uses: actions/checkout@{CHECKOUT_SHA} # v6.0.2
        with:
          persist-credentials: false
      - name: Build production image
        run: docker build --pull --tag application:test .
      - name: Assert non-root runtime
        run: test "$(docker image inspect application:test --format '{{{{.Config.User}}}}')" = "10001:10001"
"#
    )
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
        assert!(!source.contains("mvn "), "wrapper only: {source}");
    }

    #[test]
    fn existing_projects_without_a_wrapper_get_a_pinned_maven_builder() {
        let source = dockerfile(25, false);
        assert!(
            source.contains("FROM maven:3.9.16-eclipse-temurin-25 AS build"),
            "{source}"
        );
        assert!(source.contains("RUN mvn -B -ntp"), "{source}");
        assert!(ci_workflow(25, false).contains("run: mvn -B -ntp clean verify"));
    }
}
