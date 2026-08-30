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
const READER_MAIN_ROOT: &str = "src/main/java/";

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

/// The one dispatcher this project has, or `None` when it has none or several.
///
/// Found by shape, not by filename: `jails_codemod::dispatch::is_dispatcher`
/// checks for the registry type *and* the `return commands;` anchor, so
/// `App.java` from `new-cli` and `<Name>Cli.java` from `g cli` both qualify
/// and a class that merely happens to be called `App` does not.
///
/// `None` for "several" is deliberate. A project with two dispatchers has two
/// answers, and picking one silently is how a jar and `jails run` came to
/// start different classes.
pub(crate) fn command_dispatcher(snapshot: &WorkspaceSnapshot) -> Option<ProjectPath> {
    let mut found = snapshot
        .files
        .iter()
        .filter(|(path, _)| {
            path.as_str().starts_with(READER_MAIN_ROOT) && path.as_str().ends_with(".java")
        })
        .filter(|(_, file)| {
            std::str::from_utf8(&file.bytes).is_ok_and(jails_codemod::dispatch::is_dispatcher)
        })
        .map(|(path, _)| path.clone());
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// The dispatcher with one registration line spliced above `return commands;`.
///
/// **The "already registered" check reads blanked source**, for the same
/// reason `is_spring_boot_test` does: the dispatcher jails writes carries a
/// Javadoc example, `commands.put(ImportCommand.NAME, ImportCommand::run);`,
/// and a raw `contains` reads that comment as a registration. So
/// `jails g command Import` -- the one name the example happens to use --
/// silently registered nothing, and `g cli` then moved the entry point out
/// from under a dispatcher it believed was in use. Blanking replaces comments
/// with spaces of the same length, so the example cannot be mistaken for code.
pub(crate) fn ensure_command_registration(text: &str, class: &str, package: &str) -> String {
    if jails_codemod::text::blanked(text).contains(&format!("commands.put({class}.NAME")) {
        return text.to_string();
    }
    let import = if text.contains(&format!("\npackage {package};"))
        || text.starts_with(&format!("package {package};"))
    {
        String::new()
    } else {
        format!("import {package}.{class};\n")
    };
    jails_codemod::dispatch::splice_registration(text, class, &import)
        .unwrap_or_else(|| text.to_string())
}

/// Point `<mainClass>` at `class`, leaving every other byte alone.
///
/// Unchanged when the POM declares no main class -- a Spring Boot project,
/// where the plugin finds `@SpringBootApplication` itself.
pub(crate) fn set_maven_main_class(pom: &str, class: &str) -> String {
    const OPEN: &str = "<mainClass>";
    let Some(start) = pom.find(OPEN).map(|at| at + OPEN.len()) else {
        return pom.to_string();
    };
    let Some(end) = pom[start..].find("</mainClass>").map(|at| at + start) else {
        return pom.to_string();
    };
    format!("{}{class}{}", &pom[..start], &pom[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dispatcher `new-cli` writes, abbreviated to the parts that matter:
    /// a Javadoc example naming `ImportCommand`, and the `return commands;`
    /// anchor the registration goes above.
    fn dispatcher() -> String {
        [
            "package com.example.demo;",
            "",
            "import java.util.LinkedHashMap;",
            "import java.util.SequencedMap;",
            "",
            "public final class App {",
            "    /**",
            "     * Add yours here, for example:",
            "     * commands.put(ImportCommand.NAME, ImportCommand::run);",
            "     */",
            "    public static SequencedMap<String, Command> commands() {",
            "        var commands = new LinkedHashMap<String, Command>();",
            "        return commands;",
            "    }",
            "}",
            "",
        ]
        .join("\n")
    }

    /// **The example in the dispatcher's own Javadoc is not a registration.**
    ///
    /// `new-cli` ships a dispatcher whose comment demonstrates the call with
    /// `ImportCommand`, so a raw `contains` reported that exact class as
    /// already registered. `jails g command Import` then wrote the command and
    /// spliced nothing, and because the dispatcher still registered nothing, a
    /// later `g cli` moved `<mainClass>` out from under it -- the one thing
    /// `entry_point` exists to prevent.
    #[test]
    fn a_javadoc_example_is_not_read_as_an_existing_registration() {
        let registered =
            ensure_command_registration(&dispatcher(), "ImportCommand", "com.example.demo.cli");
        assert!(
            registered.contains("        commands.put(ImportCommand.NAME, ImportCommand::run);"),
            "the Javadoc example suppressed the registration:\n{registered}"
        );
    }

    /// And a real registration still suppresses a second one, so the splice
    /// stays idempotent across re-planning.
    #[test]
    fn an_existing_registration_is_not_written_twice() {
        let once =
            ensure_command_registration(&dispatcher(), "ImportCommand", "com.example.demo.cli");
        let twice = ensure_command_registration(&once, "ImportCommand", "com.example.demo.cli");
        assert_eq!(once, twice);
    }
}
