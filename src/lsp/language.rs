//! What JDL offers at a cursor: the completions, the hover and the jump.
//!
//! **Two sources and no third.** The static half is
//! [`jails_model::jdl_grammar`], which is the table the parser refuses from
//! and `jails explain jdl` prints, so a family that gains an attribute gains
//! it here in the same change. The other half is the model the buffer links
//! to, which is what makes `uuid` and `Payout` one list rather than two.
//!
//! **The buffer decides the family, not a CST.** A reader typing `@` is
//! mid-declaration and the document does not parse yet, which is precisely
//! when they want the list. So the enclosing family comes from the line's
//! own first word and the indentation above it -- a scan the half-typed text
//! survives, where a parse does not.

use super::document::{before, line_at, word_at};
use jails_model::StableId as _;
use jails_model::jdl_grammar::{self, Family};
use serde_json::{Value, json};
use std::path::Path;

/// The language's own diagnostics, from the same parse and link
/// `jails model check` runs, in the protocol's shape.
pub(super) fn diagnostics(text: &str) -> Vec<Value> {
    let Err(found) = jails_model::parse_jdl(text) else {
        return Vec::new();
    };
    found
        .diagnostics
        .iter()
        .map(|diagnostic| {
            // One-based in the model, zero-based here. A diagnostic with no
            // location is about no line -- a collision between two
            // declarations is about neither -- and the top of the file is
            // where the protocol puts one, because it has nowhere else.
            let line = diagnostic.line.unwrap_or(1).saturating_sub(1);
            let column = diagnostic.column.unwrap_or(1).saturating_sub(1);
            json!({
                "range": {
                    "start": { "line": line, "character": column },
                    "end": { "line": line, "character": column },
                },
                "severity": match diagnostic.severity {
                    jails_model::Severity::Warning => 2,
                    jails_model::Severity::Error => 1,
                },
                "code": diagnostic.code,
                "source": "jails",
                // The fix travels with the message: an editor shows one
                // string, and a diagnostic that says what is wrong without
                // saying what to do is half an answer everywhere else in
                // this tool.
                "message": format!("{}\nfix: {}", diagnostic.message, diagnostic.fix),
            })
        })
        .collect()
}

/// What may be typed at this cursor.
pub(super) fn completion(text: &str, line: usize, column: usize) -> Vec<Value> {
    let Some(source) = line_at(text, line) else {
        return Vec::new();
    };
    let head = before(source, column);
    if let Some(prefix) = attribute_prefix(head) {
        return attributes(text, line, prefix);
    }
    if let Some(prefix) = type_prefix(head) {
        return types(text, prefix);
    }
    if head.trim().is_empty() {
        return keywords(text, line, head);
    }
    Vec::new()
}

/// The `@…` the cursor is finishing, if it is finishing one.
fn attribute_prefix(head: &str) -> Option<&str> {
    let at = head.rfind('@')?;
    let prefix = &head[at + 1..];
    prefix
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_')
        .then_some(prefix)
}

/// The type the cursor is finishing, if the line has a colon behind it and
/// nothing but the type after.
fn type_prefix(head: &str) -> Option<&str> {
    let at = head.rfind(':')?;
    let prefix = head[at + 1..].trim_start();
    // A space after a finished word means the reader has moved on to the
    // markers; only the word directly after the colon is a type.
    (!prefix.contains(' ')
        && prefix
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_'))
    .then_some(prefix)
}

fn attributes(text: &str, line: usize, prefix: &str) -> Vec<Value> {
    let Some(family) = enclosing(text, line) else {
        return Vec::new();
    };
    family
        .attributes
        .iter()
        .filter(|attribute| attribute.starts_with(prefix))
        .map(|attribute| {
            item(
                attribute,
                // `Keyword` in the protocol's enumeration: an attribute is
                // language, not a value the project supplies.
                14,
                &format!("`@{attribute}` on {}", subject(family)),
            )
        })
        .collect()
}

/// Every type a component may take: the language's own, then the entity and
/// enum names this buffer declares.
///
/// Capitalised means the project's, which is the rule rather than a
/// convention, so offering `Payout` beside `uuid` is offering the two halves
/// of one closed answer.
fn types(text: &str, prefix: &str) -> Vec<Value> {
    let builtins = jails_model::builtin::ALL.iter().map(|(_, row)| {
        (
            row.token.to_string(),
            format!("`{}` in Java", row.java_boxed),
        )
    });
    // Declared names come off the text rather than a link, because a buffer
    // being typed into does not link -- and the entity two lines up is
    // exactly the one the reader is about to reference.
    let declared = declared_types(text)
        .into_iter()
        .map(|name| (name, "declared in this model".to_string()));
    builtins
        .chain(declared)
        .filter(|(name, _)| name.starts_with(prefix))
        // `Struct` and `Value` in the protocol's enumeration would both be
        // guesses; `Class` is what a type is in every other language server.
        .map(|(name, detail)| item(&name, 7, &detail))
        .collect()
}

/// The declaration keywords valid at this indentation.
fn keywords(text: &str, line: usize, head: &str) -> Vec<Value> {
    let inside = enclosing_keyword(text, line).is_some();
    let indented = !head.is_empty();
    jdl_grammar::FAMILIES
        .iter()
        // A family whose keyword the grammar writes as `<field>` is a shape
        // rather than a word, and there is nothing to complete.
        .filter(|family| !family.keyword.contains('<'))
        .filter(|family| family.keyword.starts_with(' ') == (inside && indented))
        .map(|family| item(family.keyword.trim(), 14, family.summary))
        .collect()
}

/// What the cursor's word means, as markdown, or nothing.
pub(super) fn hover(text: &str, line: usize, column: usize) -> Option<String> {
    let source = line_at(text, line)?;
    let word = word_at(source, column);
    if word.is_empty() {
        return None;
    }
    if let Some(attribute) = word.strip_prefix('@') {
        let on: Vec<&str> = jdl_grammar::FAMILIES
            .iter()
            .filter(|family| family.attributes.contains(&attribute))
            .map(|family| family.keyword.trim())
            .collect();
        if on.is_empty() {
            return None;
        }
        return Some(format!(
            "`@{attribute}` — valid on {}",
            on.iter()
                .map(|keyword| format!("`{keyword}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(family) = jdl_grammar::FAMILIES
        .iter()
        .find(|family| family.keyword.trim() == word)
    {
        return Some(format!("`{word}` — {}", family.summary));
    }
    if let Some((_, row)) = jails_model::builtin::ALL
        .iter()
        .find(|(_, row)| row.token == word)
    {
        return Some(format!(
            "`{word}` — `{}` in Java, `{}` in PostgreSQL",
            row.java_boxed, row.sql_postgres
        ));
    }
    None
}

/// Every generated file the declaration under the cursor produced.
///
/// **The lock already knows, and nothing is recompiled to ask it.** Each
/// accepted file carries the semantic ids it came from, so the jump from a
/// declaration to its output is a lookup rather than a plan -- which is also
/// why it is exact: a file that names this entity's id is one this entity
/// caused, and a file that does not is not.
pub(super) fn definition(
    text: &str,
    line: usize,
    column: usize,
    project: &crate::project::Project,
) -> Vec<Value> {
    let root = project.root();
    let Some(source) = line_at(text, line) else {
        return Vec::new();
    };
    let word = word_at(source, column);
    if word.is_empty() {
        return Vec::new();
    }
    let Ok(model) = jails_model::parse_jdl(text) else {
        return Vec::new();
    };
    let Some(id) = stable_id(&model, word) else {
        return Vec::new();
    };
    let snapshot = jails_project::capture::capture(
        root,
        Path::new(jails_model::MODEL_FILE),
        text.as_bytes(),
        model,
        None,
        &[],
        jails_project::capture::ModelFile::Observed,
    );
    let Ok(snapshot) = snapshot else {
        return Vec::new();
    };
    snapshot
        .accepted_projection
        .as_ref()
        .into_iter()
        .flat_map(|projection| projection.files.iter())
        .filter(|(_, file)| file.provenance.semantic_ids.contains(&id))
        .map(|(path, _)| {
            json!({
                "uri": format!("file://{}", root.join(path.as_str()).display()),
                // The whole file, because a generated file is the answer and
                // a line inside it would be a guess about which.
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 0 },
                },
            })
        })
        .collect()
}

/// The stable id of the declaration this word names, if it names one.
fn stable_id(model: &jails_model::AppModel, word: &str) -> Option<String> {
    if let Some((id, _)) = model
        .entities
        .iter()
        .find(|(_, entity)| entity.names.java_type == word || entity.label == word)
    {
        return Some(id.as_str().to_string());
    }
    if let Some((id, _)) = model
        .components
        .iter()
        .find(|(_, component)| component.name == word || component.label == word)
    {
        return Some(id.as_str().to_string());
    }
    None
}

/// Every entity and enum name this buffer declares, read off the text.
fn declared_types(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let keyword = words.next()?;
            if !matches!(keyword, "entity" | "enum" | "component") {
                return None;
            }
            // `component <kind> <Name>` carries a kind between the two.
            let name = if keyword == "component" {
                words.nth(1)?
            } else {
                words.next()?
            };
            let name = name.trim_end_matches('{').trim();
            name.chars()
                .next()
                .is_some_and(char::is_uppercase)
                .then(|| name.to_string())
        })
        .collect()
}

/// Which family the cursor's line belongs to.
///
/// The line's own first word decides it where the grammar names one; where
/// it does not, the shape does -- a `name: type` line inside an `entity` is
/// a field, and the same shape inside an operation is an input.
fn enclosing(text: &str, line: usize) -> Option<&'static Family> {
    let source = line_at(text, line)?;
    let word = source.split_whitespace().next().unwrap_or("");
    let indent = indent_of(source);
    if indent == 0 {
        return family(word);
    }
    match word.trim_end_matches(&['(', '{'][..]) {
        "index" => return family("  index"),
        "relation" => return family("  relation"),
        "command" | "query" | "transition" | "event" => return family("  <operation>"),
        _ => {}
    }
    // A `name: type` line, whose family is decided by what it is inside.
    match enclosing_keyword(text, line) {
        Some("entity") => family("  <field>"),
        Some("command" | "query" | "transition" | "event") => family("    <input>"),
        Some("enum") => family("enum"),
        _ => None,
    }
}

/// The first word of the nearest line above this one with a smaller indent.
fn enclosing_keyword(text: &str, line: usize) -> Option<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let indent = indent_of(lines.get(line)?);
    lines[..line]
        .iter()
        .rev()
        .filter(|source| !source.trim().is_empty())
        .find(|source| indent_of(source) < indent)
        .and_then(|source| source.split_whitespace().next())
        .map(|word| word.trim_end_matches(&['(', '{'][..]))
}

/// A family named for a reader rather than for the grammar.
///
/// The table writes a field as `  <field>` because it is a shape and not a
/// word, which is right in `explain jdl`'s printed list and wrong in a
/// completion popup, where the angle brackets read as a placeholder the
/// reader is meant to fill in.
fn subject(family: &Family) -> String {
    let keyword = family.keyword.trim();
    match keyword
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    {
        Some(shape) => format!("a{} {shape}", if shape == "input" { "n" } else { "" }),
        None => format!("`{keyword}`"),
    }
}

fn family(keyword: &str) -> Option<&'static Family> {
    jdl_grammar::FAMILIES
        .iter()
        .find(|family| family.keyword == keyword)
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn item(label: &str, kind: u8, detail: &str) -> Value {
    json!({ "label": label, "kind": kind, "detail": detail })
}
