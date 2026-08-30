//! Putting `@Import(TestcontainersConfig.class)` on tests the reader owns.
//!
//! **Why this edit has to exist at all.** The moment
//! `spring-boot-starter-jdbc` is in the build, JDBC auto-configuration demands
//! a `DataSource` for *every* `@SpringBootTest` in the project — including the
//! `contextLoads` test that shipped with it and never touches a database. So a
//! project that declares `storage postgres` and touches nothing else fails
//! `mvn verify` on a test nobody wrote.
//!
//! **Why it is an import rather than a global registration.** jails used to
//! register the container from a test-classpath `spring.factories`, which gave
//! every `@SpringBootTest` a `DataSource` for free — and made every pure slice
//! and every `@WebMvcTest` start a PostgreSQL it never queried. Naming it per
//! test is the cost of not doing that.
//!
//! **The target set comes from the snapshot, never from disk.** A file this
//! touches has a captured before-image, so the plan is exact: a test edited
//! after review makes the plan stale rather than being silently overwritten.

use jails_contracts::{ProjectPath, WorkspaceSnapshot};

const TEST_SOURCE_ROOT: &str = "src/test/java/";

/// Every captured `@SpringBootTest` that does not already import `class`.
///
/// Ordered by path, so a plan built twice from one snapshot is one plan.
pub(crate) fn spring_boot_test_targets(
    snapshot: &WorkspaceSnapshot,
    class: &str,
) -> Vec<ProjectPath> {
    snapshot
        .files
        .iter()
        .filter(|(path, _)| {
            path.as_str().starts_with(TEST_SOURCE_ROOT) && path.as_str().ends_with(".java")
        })
        .filter_map(|(path, file)| {
            let text = std::str::from_utf8(&file.bytes).ok()?;
            // The config class carries `@TestConfiguration`, not
            // `@SpringBootTest`, so it cannot select itself -- but a reader
            // who wrote their own is excluded by name as well.
            (jails_codemod::annotate::is_spring_boot_test(text)
                && !text.contains(&format!("@Import({class}.class)")))
            .then(|| path.clone())
        })
        .collect()
}

/// One test, with the annotation and its import spliced in.
///
/// Returns the source unchanged when there is no `@SpringBootTest` to anchor
/// to, which `spring_boot_test_targets` has already ruled out — the fallback
/// exists so a race between capture and materialize cannot corrupt a file.
pub(crate) fn ensure_spring_test_import(text: &str, class: &str, package: &str) -> String {
    let extra = if text.starts_with(&format!("package {package};"))
        || text.contains(&format!("\npackage {package};"))
    {
        // Same package: importing a sibling is a compile error.
        String::new()
    } else {
        format!("import {package}.{class};\n")
    };
    jails_codemod::annotate::splice_import(text, class, &extra).unwrap_or_else(|| text.to_string())
}
