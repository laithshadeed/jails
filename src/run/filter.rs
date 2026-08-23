//! What a person typed, or where their cursor is, turned into a JUnit selector.
//!
//! Four shapes, and the last is why this exists at all: **JUnit has no
//! file-and-line selector**. `src/test/java/.../PayoutTest.java:42` is what an
//! editor keybinding has to hand, so something must resolve the `@Test` that
//! encloses that line -- and if the tool does not, every editor integration
//! reimplements it.
//!
//! The other three: a bare `Money` becomes `MoneyTest`; `Money#converts` runs
//! one method, with the suffix applied to the **class half only**; and a name
//! ending in `IT` is Failsafe's rather than Surefire's. That last decision is
//! made on the class, not on the finished string -- `PayoutIT#settles` ends in
//! `settles`, so routing on the whole filter sent an integration test to
//! Surefire, which does not run `*IT`, and Maven reported success having
//! executed nothing.

use super::*;

/// Split `Class#method` into its two halves. Anything with no `#` is all
/// class.
pub(super) fn split_method(filter: &str) -> (&str, Option<&str>) {
    match filter.split_once('#') {
        Some((class, method)) => (class, Some(method)),
        None => (filter, None),
    }
}

/// `Payout` -> `PayoutTest`, `Payout#settles` -> `PayoutTest#settles`.
///
/// The suffix belongs to the class alone. Appending it to the whole filter
/// produced `Payout#settlesTest`, a method nothing declares, and Surefire
/// then failed the build for a filter jails itself had corrupted.
pub(super) fn expand_filter(filter: &str) -> String {
    let (class, method) = split_method(filter);
    let expanded = if class.ends_with("Test")
        || class.ends_with("Tests")
        || class.ends_with("IT")
        || class.contains('*')
        // `Outer$Nested` and `com.example.PayoutTest` are already fully
        // specified. Applying the bare-name convention to them produced
        // `PayoutTest$WhenDeclinedTest`, a class nothing declares -- which
        // is exactly the shape `jails test <file>:<line>` resolves to.
        || class.contains('$')
        || class.contains('.')
    {
        class.to_string()
    } else {
        format!("{class}Test")
    };
    match method {
        Some(method) => format!("{expanded}#{method}"),
        None => expanded,
    }
}

pub(super) fn resolve_filter(root: &Path, filter: &str) -> Result<String> {
    let Some((path, line)) = split_file_line(filter) else {
        return Ok(filter.to_string());
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let source = fs::read_to_string(&absolute)
        .map_err(|e| format!("failed to read {}: {e}", absolute.display()))?;
    let class = enclosing_class(&source, &absolute);
    match enclosing_test_method(&source, line) {
        Some(method) => Ok(format!("{class}#{method}")),
        // A line between methods, or on the class declaration, is a
        // reasonable thing to have the cursor on. Running the class is what
        // the reader meant, and saying so beats refusing.
        None => {
            println!(
                "jails: no @Test encloses {}:{line} -- running {class}",
                path.display()
            );
            Ok(class)
        }
    }
}

/// `src/test/java/com/example/PayoutTest.java:42` -> the two halves.
///
/// Split from the right, because a Windows path starts `C:\`.
pub(super) fn split_file_line(filter: &str) -> Option<(&Path, usize)> {
    let (path, line) = filter.rsplit_once(':')?;
    let line = line.parse().ok()?;
    let path = Path::new(path);
    path.extension()
        .is_some_and(|e| e == "java")
        .then_some((path, line))
}

/// The class a `-Dtest` filter should name for this file.
///
/// The file stem, unless the line sits inside a `@Nested` class, which
/// JUnit addresses as `Outer$Nested`.
pub(super) fn enclosing_class(source: &str, path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let text = crate::java::blanked(source);
    let mut nested: Option<String> = None;
    for line in text.lines() {
        if let Some(name) = declared_type_name(line)
            && name != stem
        {
            nested = Some(name);
        }
    }
    match nested {
        Some(inner) => format!("{stem}${inner}"),
        None => stem,
    }
}

/// `    static class Deletes {` -> `Deletes`.
pub(super) fn declared_type_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for keyword in ["class ", "record ", "interface "] {
        if let Some(at) = trimmed.find(keyword) {
            // Only a declaration, not `new Foo() { class ...` inside an
            // expression: everything before the keyword has to be modifiers.
            if trimmed[..at].split_whitespace().all(|word| {
                matches!(
                    word,
                    "public"
                        | "private"
                        | "protected"
                        | "static"
                        | "final"
                        | "abstract"
                        | "sealed"
                        | "non-sealed"
                )
            }) {
                let rest = &trimmed[at + keyword.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// The name of the `@Test` method containing `line` (1-based).
///
/// Scans upward for a method declaration and then checks that a test
/// annotation sits above it, which is the same shape `java::annotations`
/// reads -- but line numbers matter here and that reader does not carry
/// them. Comments and string literals are blanked first, so a `@Test` in
/// Javadoc cannot promote the method below it.
pub(super) fn enclosing_test_method(source: &str, line: usize) -> Option<String> {
    let text = crate::java::blanked(source);
    let lines: Vec<&str> = text.lines().collect();
    let start = line.min(lines.len()).checked_sub(1)?;
    for index in (0..=start).rev() {
        let Some(name) = method_name(lines[index]) else {
            continue;
        };
        let annotated = lines[..index]
            .iter()
            .rev()
            // Annotations and modifiers may sit between; a blank line or
            // another statement means this method has none of its own.
            .take_while(|l| {
                let t = l.trim();
                t.starts_with('@') || t.is_empty() || t.starts_with("//")
            })
            .any(|l| is_test_annotation(l));
        if annotated {
            return Some(name);
        }
    }
    None
}

/// Is this line a JUnit test annotation?
///
/// Matched on the annotation's **last segment**, because the fully qualified
/// form is real: jails' own generated integration tests carry
/// `@org.springframework.transaction.annotation.Transactional`, and a reader
/// who writes `@org.junit.jupiter.api.Test` is not doing anything strange.
/// Matching `@Test` as a prefix missed every one of them, and the symptom was
/// `jails test <file>:<line>` quietly widening to the whole class.
pub(super) fn is_test_annotation(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix('@') else {
        return false;
    };
    let name = rest
        .split(['(', ' '])
        .next()
        .unwrap_or(rest)
        .rsplit('.')
        .next()
        .unwrap_or(rest);
    matches!(
        name,
        "Test" | "ParameterizedTest" | "RepeatedTest" | "TestFactory" | "TestTemplate"
    )
}

/// `    void settlesAPayment() {` -> `settlesAPayment`.
pub(super) fn method_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let open = trimmed.find('(')?;
    let before = &trimmed[..open];
    let name = before.split_whitespace().last()?;
    // A call is not a declaration: `assertThat(x)` has no return type in
    // front of it, and a declaration always has at least `void`.
    if before.split_whitespace().count() < 2 {
        return None;
    }
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then(|| name.to_string())
}
