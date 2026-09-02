//! The three files `new` writes by hand before the model takes over: the
//! plain-Java `App.java` and its test, and a generated Java file's
//! `package-info.java` when JSpecify is on the classpath.

use super::publish;
use jails_support::Result;
use std::path::Path;

/// Write one new Java or resource file into the reserved tree, refusing a
/// path that already exists, with imports normalised the way the compiler
/// normalises its own output.
pub(super) fn write_new_file(tree: publish::Tree<'_>, path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Err(format!(
            "{} already exists.\n       fix: choose a different name, or destroy the generated artifact first.",
            path.display()
        )
        .into());
    }
    let contents = if path.extension().is_some_and(|e| e == "java") {
        ensure_package_info(&tree, path)?;
        jails_codemod::tidy::tidy_blank_lines(&jails_codemod::tidy::normalize_imports(contents))
    } else {
        contents.to_string()
    };
    tree.create_at(path, &contents)
}

pub(super) fn cli_java(pkg: &str, class: &str, program: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_java.java"),
        &[
            ("pkg", pkg),
            ("class", class),
            ("program", program),
            ("registrations", ""),
        ],
    )
}

pub(super) fn cli_test(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_test_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

/// A `package-info.java` beside a new main-source Java file when the project
/// declares JSpecify, so the package is null-marked rather than unspecified.
fn ensure_package_info(tree: &publish::Tree<'_>, class_path: &Path) -> Result<()> {
    let Some(dir) = class_path.parent() else {
        return Ok(());
    };
    let dir_text = dir.to_string_lossy();
    if !dir_text.contains("src/main/java") {
        return Ok(());
    }
    let info = dir.join("package-info.java");
    if class_path == info || info.exists() {
        return Ok(());
    }
    let pom = crate::pom::read(tree.root()).unwrap_or_default();
    if !crate::pom::has_dependency(&pom, "org.jspecify", "jspecify") {
        return Ok(());
    }
    let Some(pkg) = dir_text
        .split("src/main/java/")
        .nth(1)
        .map(|rest| rest.trim_matches('/').replace('/', "."))
        .filter(|pkg| !pkg.is_empty())
    else {
        return Ok(());
    };
    tree.put_at(
        &info,
        format!(
            r#"/**
 * Every reference type in this package is non-null unless it is explicitly
 * annotated {{@code @Nullable}}.
 *
 * <p>This is a package-level opt-in because that is the only level JSpecify
 * offers: without it the package is "unspecified nullness" and a nullness
 * checker has nothing to check.
 */
@NullMarked
package {pkg};

import org.jspecify.annotations.NullMarked;
"#
        ),
    )
}
