//! Editing one annotation on a Java class somebody else owns.
//!
//! Specifically `@Import`, which is how a Spring test says which
//! `@TestConfiguration` it wants. `jails add db` has to put one on every
//! `@SpringBootTest` already in the project: the moment
//! `spring-boot-starter-jdbc` lands in the POM, auto-configuration demands a
//! `DataSource` for *every* one of them -- including the `contextLoads` test
//! that shipped with the project and never touches a database.
//!
//! Every edit here is surgical, because these are files the reader owns. The
//! annotation is rewritten member by member rather than replaced, and
//! [`unsplice_import`] puts it back exactly as it was.
//!
//! It is text in and text out: nothing here reads or writes a file, and
//! nothing here knows what a capability is.

/// `@Import(Class.class)`, the form this module reads and writes.
pub(crate) fn import_annotation(class: &str) -> String {
    format!("@Import({class}.class)")
}

/// The members of an `@Import(...)` line, single or braced.
fn import_members(line: &str) -> Option<Vec<String>> {
    let inner = line
        .trim()
        .strip_prefix("@Import(")?
        .strip_suffix(')')?
        .trim();
    let inner = inner
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(inner);
    Some(
        inner
            .split(',')
            .map(str::trim)
            .filter(|member| !member.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn render_import_annotation(line: &str, members: &[String]) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    if members.len() == 1 {
        format!("{indent}@Import({})", members[0])
    } else {
        format!("{indent}@Import({{{}}})", members.join(", "))
    }
}

/// Insert `@Import(Class.class)` immediately above `@SpringBootTest` and add
/// the annotation import (plus `extra` when the config lives in another
/// package). `None` when the anchor is missing.
pub fn splice_import(source: &str, class: &str, extra: &str) -> Option<String> {
    let annotation = import_annotation(class);
    // Located through `blanked`, and the offset then used against `source`.
    // A raw `source.find` matches the `@SpringBootTest` inside a Javadoc
    // example -- which `TestcontainersConfig`'s own docs contain -- and would
    // splice the `@Import` into the middle of a comment. `is_spring_boot_test`
    // reads through `blanked` too; the two have to agree about where an
    // annotation is, or one decides a file qualifies and the other decides
    // where to edit it.
    let anchor = crate::text::blanked(source).find("@SpringBootTest")?;
    let line_start = source[..anchor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let target = format!("{class}.class");

    // `@Import` may sit before or after `@SpringBootTest`; both orders are
    // legal and generated tests use both. It is not repeatable, so merge into
    // the existing annotation rather than adding a second one.
    let existing = source
        .lines()
        .find(|line| line.trim_start().starts_with("@Import("));
    let mut out = if let Some(line) = existing {
        let mut members = import_members(line)?;
        if !members.iter().any(|member| member == &target) {
            members.push(target);
        }
        source.replacen(line, &render_import_annotation(line, &members), 1)
    } else {
        let mut out = String::with_capacity(source.len() + annotation.len() + extra.len() + 64);
        out.push_str(&source[..line_start]);
        out.push_str(&annotation);
        out.push('\n');
        out.push_str(&source[line_start..]);
        out
    };

    let mut imports = String::new();
    if !out.contains("org.springframework.context.annotation.Import") {
        imports.push_str("import org.springframework.context.annotation.Import;\n");
    }
    imports.push_str(extra);
    if !imports.is_empty() {
        let package_end = out.find(";\n").map(|i| i + 2)?;
        let mut with_import = String::with_capacity(out.len() + imports.len());
        with_import.push_str(&out[..package_end]);
        with_import.push('\n');
        with_import.push_str(&imports);
        with_import.push_str(&out[package_end..]);
        out = with_import;
    }
    Some(crate::tidy::normalize_imports(&out))
}

/// Take one member back out of `@Import`, and the import statements that
/// existed only for it. `None` when this class was not imported here.
pub fn unsplice_import(source: &str, class: &str, extra: &str) -> Option<String> {
    let target = format!("{class}.class");
    let extra = extra.trim();
    let mut removed = false;
    let mut lines = Vec::new();
    for line in source.lines() {
        if let Some(mut members) = import_members(line) {
            let before = members.len();
            members.retain(|member| member != &target);
            if members.len() != before {
                removed = true;
                if !members.is_empty() {
                    lines.push(render_import_annotation(line, &members));
                }
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if !removed {
        return None;
    }
    let dropping_import_stmt = !lines
        .iter()
        .any(|line| line.trim_start().starts_with("@Import("));
    lines.retain(|line| {
        let trimmed = line.trim();
        if !extra.is_empty() && trimmed == extra {
            return false;
        }
        !(dropping_import_stmt
            && trimmed == "import org.springframework.context.annotation.Import;")
    });
    let mut out = lines.join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    Some(crate::tidy::normalize_imports(&out))
}

/// Whether this source declares a `@SpringBootTest`.
///
/// Read through [`crate::text::blanked`]: a raw substring search finds
/// `@SpringBootTest` inside a Javadoc example -- `TestcontainersConfig`'s own
/// Javadoc contains exactly that -- and counts the container config as a test
/// needing the config imported into itself.
///
/// It answers a narrower question than `java::types_annotated_with`, which
/// also checks that the annotation sits on the *top-level type*. Here the
/// looser answer is the right one: `splice_import` anchors on the first
/// `@SpringBootTest` it finds, so what matters is whether there is one to
/// anchor to.
pub fn is_spring_boot_test(source: &str) -> bool {
    crate::text::blanked(source).contains("@SpringBootTest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_import_lands_above_spring_boot_test_and_is_idempotent_to_unsplice() {
        let source = r#"package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
class DemoApplicationTests {

    @Test
    void contextLoads() {}
}
"#;
        let spliced = splice_import(source, "PostgresContainerConfig", "").unwrap();
        assert!(spliced.contains("@Import(PostgresContainerConfig.class)"));
        assert!(spliced.contains("import org.springframework.context.annotation.Import;"));
        let import_at = spliced
            .find("@Import(PostgresContainerConfig.class)")
            .unwrap();
        let boot_at = spliced.find("@SpringBootTest").unwrap();
        assert!(import_at < boot_at, "{spliced}");

        let restored = unsplice_import(&spliced, "PostgresContainerConfig", "").unwrap();
        assert!(!restored.contains("PostgresContainerConfig"));
        assert!(!restored.contains("org.springframework.context.annotation.Import"));
        assert!(restored.contains("@SpringBootTest"));

        let extra = "import com.example.demo.testkit.PostgresContainerConfig;\n";
        let other_pkg = splice_import(source, "PostgresContainerConfig", extra).unwrap();
        assert!(other_pkg.contains("import com.example.demo.testkit.PostgresContainerConfig;"));
        let round_trip = unsplice_import(&other_pkg, "PostgresContainerConfig", extra).unwrap();
        assert!(!round_trip.contains("testkit.PostgresContainerConfig"));
    }

    /// A `@SpringBootTest` written in a Javadoc example is prose; anchoring
    /// on it puts `@Import(...)` inside the comment, where it annotates
    /// nothing and the class it was meant for still has no container.
    #[test]
    fn splice_import_ignores_a_spring_boot_test_named_in_a_comment() {
        let source = r#"package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

/**
 * Boots the whole context.
 *
 * <p>Written as {@code @SpringBootTest} rather than a slice, because the
 * point is that every bean wires.
 */
@SpringBootTest
class DemoApplicationTests {

    @Test
    void contextLoads() {}
}
"#;
        let spliced = splice_import(source, "TestcontainersConfig", "").unwrap();
        let import_at = spliced.find("@Import(TestcontainersConfig.class)").unwrap();
        let comment_end = spliced.find(" */").unwrap();
        assert!(import_at > comment_end, "{spliced}");
        assert!(
            spliced.contains("@Import(TestcontainersConfig.class)\n@SpringBootTest\nclass"),
            "{spliced}"
        );
        assert_eq!(
            unsplice_import(&spliced, "TestcontainersConfig", "").unwrap(),
            source
        );
    }

    #[test]
    fn splice_merges_with_an_existing_spring_import_and_unsplice_preserves_it() {
        let source = r#"package com.example.demo;

import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@SpringBootTest
@Import(TransactionMessagingIT.Containers.class)
class TransactionMessagingIT {}
"#;
        let spliced = splice_import(source, "PostgresContainerConfig", "").unwrap();
        assert!(
            spliced.contains(
                "@Import({TransactionMessagingIT.Containers.class, PostgresContainerConfig.class})"
            ),
            "{spliced}"
        );
        assert_eq!(spliced.matches("@Import(").count(), 1, "{spliced}");

        let restored = unsplice_import(&spliced, "PostgresContainerConfig", "").unwrap();
        assert!(
            restored.contains("@Import(TransactionMessagingIT.Containers.class)"),
            "{restored}"
        );
        assert!(restored.contains("org.springframework.context.annotation.Import"));
    }
}
