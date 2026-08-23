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
//!
//! ## Overrides, and the guarantee they cost
//!
//! `plan.md` §6.6 tier 2: "change what the generated code *looks like*" is the
//! flexibility people actually reach for -- not a new generator, just *this*
//! class shaped differently. A file at `.jails/templates/<name>` (project) or
//! `~/.config/jails/templates/<name>` (machine) replaces the built-in of the
//! same name, in that order. The precedent is `openapi-generator`'s
//! `-t/--template-dir`.
//!
//! **An overridden template is not golden-tested.** That is the honest cost,
//! and it is why `doctor` reports every active override by name -- the same
//! rule as `remove`'s `unowned_properties`: jails names what it did not write
//! before the reader has to find out from a failing build.
//!
//! Overrides are held to the same contract as the built-ins: the placeholder
//! set must match exactly. A template missing a key the generator supplies, or
//! using one it does not, is an **error naming the file** -- not a panic, since
//! this is now a user-authored file and a panic would report jails' bug for
//! the reader's typo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where overrides are looked for, in order. Empty until [`install`] runs.
static OVERRIDES: OnceLock<BTreeMap<String, Override>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Override {
    pub path: PathBuf,
    /// Deliberately not `contents`: `tests/architecture.rs` counts that field
    /// name as a file-about-to-be-written, and rung 2's gate is that there is
    /// exactly one such struct. This is a file already *read*.
    text: String,
}

/// Name a template by its path under a crate's template root, resolving any
/// override.
///
/// The `include_str!` default is still a compile-time constant, so a project
/// with no overrides does no file access at all and cannot be made to fail by
/// a missing template.
///
/// `$dir` is the caller's template root and is not optional, because
/// `CARGO_MANIFEST_DIR` expands at the *call site*: a macro that baked it in
/// resolved to whichever crate invoked it, which is the workspace's one real
/// trap here. Each crate declares its own one-line `template!` wrapper naming
/// its root, so a wrong path is a compile error rather than a silent miss.
#[macro_export]
macro_rules! template_at {
    ($dir:expr, $name:literal) => {
        $crate::template::resolve($name, include_str!(concat!($dir, $name)))
    };
}

/// Point the override search at a project. Idempotent; the first call wins.
///
/// Called from `Project::load`/`Project::inspect`, so every command that has
/// resolved a project has the right overrides and nothing else has to remember.
/// `new` runs before a project exists, so it sees the machine tier only --
/// which is the correct answer, not an omission.
pub fn install(root: &Path) {
    let _ = OVERRIDES.set(discover(user_templates(), Some(root)));
}

/// Every override in effect, for `doctor` to name.
pub fn active() -> Vec<(String, PathBuf)> {
    OVERRIDES
        .get()
        .map(|found| {
            found
                .iter()
                .map(|(name, entry)| (name.clone(), entry.path.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// The built-in text, or an override's, for one template name.
///
/// An override is held to the built-in's placeholder set. Falling back to the
/// built-in on a mismatch would be the worst of the three options -- the
/// reader's file is ignored and the build is green -- so this refuses, naming
/// the file and the exact difference.
pub fn resolve(name: &'static str, default: &'static str) -> String {
    let Some(entry) = OVERRIDES.get().and_then(|found| found.get(name)) else {
        return default.to_string();
    };
    if let Err(problem) = agrees(&entry.text, default) {
        // Not an `assert!`: the built-ins' placeholders are jails' bug to fix
        // and a panic reports them correctly, but this file is the reader's,
        // and handing them a panic for their own typo names the wrong author.
        eprintln!(
            "jails: template override {} does not match the built-in `{name}`.\n       \
             {problem}\n       \
             fix: the placeholders are the contract -- copy jails' own \
             templates/{name} and edit around them.",
            entry.path.display()
        );
        std::process::exit(2);
    }
    entry.text.clone()
}

/// Whether an override uses exactly the built-in's placeholders.
fn agrees(candidate: &str, default: &str) -> std::result::Result<(), String> {
    let expected = placeholders(default);
    let found = placeholders(candidate);
    let missing = expected
        .iter()
        .filter(|key| !found.contains(key))
        .copied()
        .collect::<Vec<_>>();
    let unknown = found
        .iter()
        .filter(|key| !expected.contains(key))
        .copied()
        .collect::<Vec<_>>();
    match (missing.is_empty(), unknown.is_empty()) {
        (true, true) => Ok(()),
        _ => Err(format!(
            "missing: [{}]; not supplied by jails: [{}]",
            missing.join(", "),
            unknown.join(", ")
        )),
    }
}

/// The two tiers, taken as arguments so a test can exercise the precedence
/// without setting a process-global environment variable -- every unit test in
/// this crate shares one process (`CLAUDE.md`), so a test that sets `HOME` or
/// `XDG_CONFIG_HOME` races every other test that reads it.
fn discover(user: Option<PathBuf>, root: Option<&Path>) -> BTreeMap<String, Override> {
    let mut found = BTreeMap::new();
    // Machine tier first, project tier second: the later insert wins, which is
    // the documented order (a project's override beats the machine's).
    for dir in [user, root.map(|root| root.join(".jails/templates"))]
        .into_iter()
        .flatten()
    {
        collect(&dir, &dir, &mut found);
    }
    found
}

fn user_templates() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("jails/templates"))
}

/// Walk one override directory, naming each file by its path *relative to that
/// directory* -- which is exactly how the built-ins are named.
fn collect(base: &Path, dir: &Path, found: &mut BTreeMap<String, Override>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, found);
        } else if let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(relative) = path.strip_prefix(base)
        {
            found.insert(
                relative.to_string_lossy().replace('\\', "/"),
                Override {
                    path,
                    text: contents,
                },
            );
        }
    }
}

/// Fill in a template's placeholders.
///
/// Panics if a `{{...}}` survives, which means a key was misspelled or never
/// supplied. These are jails' own templates with compile-time-known keys, so
/// that is a programming error and the golden target catches it long before a
/// user could: silently shipping `{{nmae}}` inside a generated class is the
/// one outcome worth being loud about.
pub fn render(template: impl AsRef<str>, values: &[(&str, &str)]) -> String {
    let template = template.as_ref();
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

    #[test]
    fn an_override_must_use_exactly_the_built_ins_placeholders() {
        let built_in = "package {{pkg}};\n\nclass {{name}} {}\n";
        assert!(agrees("class {{name}} in {{pkg}} {}", built_in).is_ok());

        let short = agrees("class {{name}} {}", built_in).unwrap_err();
        assert!(short.contains("missing: [pkg]"), "{short}");

        let extra = agrees("class {{name}} {{colour}} {{pkg}}", built_in).unwrap_err();
        assert!(extra.contains("not supplied by jails: [colour]"), "{extra}");
    }

    /// A project override beats a machine one, and a name is the path relative
    /// to the override root -- the same name the built-in has under
    /// `templates/`.
    #[test]
    fn a_project_override_wins_over_the_machine_one() {
        let base = jails_support::scratch::ScratchDir::in_temp("jails-tpl")
            .unwrap()
            .keep();
        let _ = std::fs::remove_dir_all(&base);
        let machine = base.join("machine/jails/templates/spring");
        let project = base.join("proj/.jails/templates/spring");
        std::fs::create_dir_all(&machine).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(machine.join("thing.java"), "machine").unwrap();
        std::fs::write(project.join("thing.java"), "project").unwrap();

        // `discover` is the pure half; `install` is the once-per-process half,
        // which a unit test must not race other tests on.
        let found = discover(
            Some(base.join("machine/jails/templates")),
            Some(&base.join("proj")),
        );
        assert_eq!(found.len(), 1, "one name, two candidates: {found:?}");
        assert_eq!(
            found.get("spring/thing.java").map(|o| o.text.as_str()),
            Some("project")
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
