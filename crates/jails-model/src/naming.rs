//! Pure projections and validators for the names exposed by the semantic model.
//!
//! Linking owns diagnostics and collision policy; this module owns only the
//! deterministic language projections. Keeping that split prevents the model
//! linker from becoming a second parser for every target language.

pub(crate) fn valid_label(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}

pub(crate) fn stable_fragment(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = false;
    for (position, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if position > 0 && !previous_was_separator {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator && !output.is_empty() {
            output.push('_');
            previous_was_separator = true;
        }
    }
    output.trim_matches('_').to_string()
}

pub(crate) fn valid_java_type(value: &str) -> bool {
    valid_java_identifier(value, Case::Upper)
}

pub(crate) fn valid_java_member(value: &str) -> bool {
    valid_java_identifier(value, Case::Lower)
}

enum Case {
    Upper,
    Lower,
}

fn valid_java_identifier(value: &str, case: Case) -> bool {
    const KEYWORDS: &[&str] = &[
        "abstract",
        "assert",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "int",
        "interface",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "strictfp",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "void",
        "volatile",
        "while",
        "true",
        "false",
        "null",
        "_",
    ];
    let mut chars = value.chars();
    let starts_well = chars.next().is_some_and(|character| match case {
        Case::Upper => character.is_ascii_uppercase(),
        Case::Lower => character.is_ascii_lowercase(),
    });
    starts_well
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !KEYWORDS.contains(&value)
}

pub(crate) fn upper_camel_case(label: &str) -> String {
    label
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

pub(crate) fn lower_camel_case(label: &str) -> String {
    let upper = upper_camel_case(label);
    let mut chars = upper.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + chars.as_str()
    })
}

pub(crate) fn snake_case(label: &str) -> String {
    label.replace('-', "_")
}

pub(crate) fn valid_route(route: &str) -> bool {
    let Some((method, path)) = route.split_once(' ') else {
        return false;
    };
    matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE")
        && path.starts_with('/')
        && path.len() > 1
        && !path.contains(char::is_whitespace)
        && !path.contains("//")
}
