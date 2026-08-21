//! Java templates, as Java files.
//!
//! Every generated Java body used to live in a Rust `format!` string. That
//! cost more than the line count suggests, because `format!` reads `{` and
//! `}` as its own syntax and Java is made of braces: **every** brace in a
//! template had to be doubled, including in Javadoc, so a class body read
//! `class {name}Controller {{` and a doc comment read `{{@code public}}`.
//! 1,172 lines across three files carried that escaping. The result was text
//! that is not Java: no syntax highlighting, no editor that can check it, and
//! a diff on a template change that is hard to read.
//!
//! A template is now a real `.java` file under `templates/`, pulled in with
//! `include_str!` so it is still a compile-time constant with no runtime file
//! access and no new dependency.
//!
//! ## The placeholder syntax, and why this one
//!
//! `{{name}}`. Chosen by checking rather than by taste: the golden snapshots
//! of all 159 generated files contain no `{{` anywhere, so the sequence
//! cannot collide with Java jails emits. The obvious alternatives both can:
//! `{name}` collides with Javadoc's `{@code ...}` shape closely enough to be
//! risky and cannot be validated (Java is full of `{`), and `${name}`
//! collides outright -- `spring.rs` generates `@Value("${...}")`.
//!
//! Because `{{` cannot appear legitimately, a leftover placeholder is
//! detectable, so an unknown or misspelled key is an **error** rather than
//! text that quietly ships inside a generated class. That is the same closed-
//! set rule `jails.toml` uses, for the same reason.
//!
//! ## Not a template engine
//!
//! Substitution only: no conditionals, no loops, no expressions. Anything
//! that varies structurally -- a Spring project's `@Component` versus a plain
//! one's absence of it, a body repeated per field -- stays in Rust and is
//! passed in as a rendered value. `refactor.md` names a general template
//! language as a non-goal, and the reason holds: a conditional in a template
//! is logic that no test can reach directly and no compiler can check.

/// Fill in a template's placeholders.
///
/// Panics if a `{{...}}` survives, which means a key was misspelled or never
/// supplied. These are jails' own templates with compile-time-known keys, so
/// that is a programming error and the golden target catches it long before a
/// user could: silently shipping `{{nmae}}` inside a generated class is the
/// one outcome worth being loud about.
pub(crate) fn render(template: &str, values: &[(&str, &str)]) -> String {
    // Checked against the *template*, never the result. A rendered value may
    // legitimately contain anything -- it is data by then -- and scanning the
    // output would make a value that happens to hold `{{` an error while
    // missing nothing extra.
    let required = placeholders(template);
    for key in &required {
        assert!(
            values.iter().any(|(k, _)| k == key),
            "template placeholder {{{{{key}}}}} was not supplied (given: {})",
            supplied(values)
        );
    }
    for (key, _) in values {
        assert!(
            required.iter().any(|k| k == key),
            "value `{key}` is not used by this template (it needs: {})",
            required.join(", ")
        );
    }

    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find("{{") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after.find("}}").unwrap_or_else(|| {
            panic!(
                "unterminated template placeholder near `{}`",
                &after[..after.len().min(30)]
            )
        });
        let key = &after[..end];
        let value = values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .expect("checked above");
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The distinct placeholder names a template uses, in first-seen order.
fn placeholders(template: &str) -> Vec<&str> {
    let mut found: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some(at) = rest.find("{{") {
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let key = &after[..end];
        if !found.contains(&key) {
            found.push(key);
        }
        rest = &after[end + 2..];
    }
    found
}

fn supplied(values: &[(&str, &str)]) -> String {
    if values.is_empty() {
        return "nothing".to_string();
    }
    values
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placeholder_is_replaced_everywhere_it_appears() {
        let out = render(
            "package {{pkg}};\n\nclass {{name}}Test {\n    {{name}} subject;\n}\n",
            &[("pkg", "com.example.demo"), ("name", "Note")],
        );
        assert_eq!(
            out,
            "package com.example.demo;\n\nclass NoteTest {\n    Note subject;\n}\n"
        );
    }

    /// The reason this syntax was chosen: Java's own braces pass through
    /// untouched, so a template is a real `.java` file and needs no escaping.
    #[test]
    fn java_braces_and_javadoc_pass_through_untouched() {
        let template =
            "/**\n * {@code public} buys nothing.\n */\nclass {{name}} {\n    void f() {}\n}\n";
        let out = render(template, &[("name", "Health")]);
        assert!(out.contains("{@code public}"), "{out}");
        assert!(out.contains("void f() {}"), "{out}");
        assert!(out.contains("class Health {"), "{out}");
    }

    /// `${...}` is generated Java (`@Value("${...}")`), which is why it is
    /// not the placeholder syntax. It must survive rendering.
    #[test]
    fn a_spring_property_placeholder_is_not_a_template_placeholder() {
        let out = render(
            "@Value(\"${app.url}\")\nString {{field}};\n",
            &[("field", "url")],
        );
        assert!(out.contains("${app.url}"), "{out}");
    }

    /// A misspelled key would otherwise ship inside a generated class, where
    /// it compiles nowhere and the reader has to guess what was meant.
    #[test]
    #[should_panic(expected = "{{nmae}} was not supplied")]
    fn a_placeholder_with_no_value_is_an_error() {
        render("class {{nmae}} {}", &[("name", "Note")]);
    }

    /// The other direction, and the one that catches a renamed placeholder:
    /// a value nothing uses means the caller believes it is doing something
    /// it is not.
    #[test]
    #[should_panic(expected = "is not used by this template")]
    fn a_value_the_template_does_not_use_is_an_error() {
        render(
            "class {{name}} {}",
            &[("name", "Note"), ("pkg", "com.example")],
        );
    }

    /// Values are substituted, not re-scanned: a value that happens to
    /// contain a placeholder-looking sequence is data, not a template.
    #[test]
    fn substitution_does_not_recurse_into_values() {
        let out = render("// {{note}}\n", &[("note", "literally {{name}} here")]);
        assert_eq!(out, "// literally {{name}} here\n");
    }
}
