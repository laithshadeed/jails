//! Emitting JSON, and nothing else.
//!
//! This lived in `project.rs`, which is `abstract.md` §3.2's clean specimen of
//! **coincidental cohesion** — the worst rung of the Yourdon–Constantine scale.
//! That module held two entirely unrelated secrets: how a Maven reactor is
//! discovered, and how a string is escaped for JSON. Nothing about the second
//! has anything to do with the first; they shared a file because the first
//! command to need JSON happened to be `about`.
//!
//! It stopped being a curiosity once nine commands emitted JSON. `why.rs`,
//! `inspect.rs`, `add/tooling.rs`, `commands.rs`, `doctor.rs` and `run.rs` were
//! all reaching into a module named after a Maven concept to borrow an escaper,
//! which is the shape that makes `project.rs` look like a dependency of things
//! that do not depend on projects.
//!
//! ## Why there is no JSON *parser* here
//!
//! jails has two dependencies and intends to keep it that way. Every payload it
//! emits is built from values it already holds, so escaping is the whole job —
//! and the one file jails *reads* in a structured format, `jails.toml`, is
//! hand-parsed against a closed key set precisely so an unknown key is an error
//! rather than silence. Adding a parser here would invite reading JSON the same
//! way, and there is nothing that needs it.

/// One string, escaped and quoted, ready to place in a JSON document.
///
/// Control characters become `\uXXXX` rather than being passed through. A raw
/// newline inside a quoted string is invalid JSON, and the values here are
/// arbitrary — a `detail` line from `doctor`, a note's text, a Java type name —
/// so "this one will not contain anything odd" is not a claim worth making.
pub(crate) fn string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// The same, or the literal `null` when there is no value.
pub(crate) fn optional_string(value: Option<&str>) -> String {
    value.map(string).unwrap_or_else(|| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_backslashes_are_escaped() {
        assert_eq!(string(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn a_raw_newline_never_reaches_the_output() {
        // A `doctor` detail or a note's text can legitimately contain one, and
        // a raw newline inside a quoted string is invalid JSON.
        let escaped = string("first\nsecond\ttabbed\r");
        assert!(!escaped.contains('\n'), "{escaped}");
        assert_eq!(escaped, r#""first\nsecond\ttabbed\r""#);
    }

    #[test]
    fn other_control_characters_become_escapes_rather_than_bytes() {
        assert_eq!(string("\u{1}"), "\"\\u0001\"");
        assert_eq!(string("\u{7f}"), "\"\\u007f\"");
    }

    #[test]
    fn absent_is_null_and_not_an_empty_string() {
        // The distinction matters to a consumer: `""` is a value the project
        // set, `null` is a fact jails could not determine.
        assert_eq!(optional_string(None), "null");
        assert_eq!(optional_string(Some("")), r#""""#);
    }

    #[test]
    fn ordinary_text_passes_through_unchanged() {
        assert_eq!(string("com.example.demo"), r#""com.example.demo""#);
    }
}
