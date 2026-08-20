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
    crate::template::render(include_str!("../../templates/add/http_server_java.java"), &[("pkg", pkg), ("class", class)])
}

pub(super) fn http_server_test_java(pkg: &str, class: &str) -> String {
    crate::template::render(include_str!("../../templates/add/http_server_test_java.java"), &[("pkg", pkg), ("class", class)])
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

