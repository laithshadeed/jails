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

/// A model label as a Java member name.
///
/// Public because the compiler projects operation parameter names with it: a
/// parameter is a *reference* to a field, spelled in the model's label
/// alphabet, and rendering that label straight into Java gives a record
/// component called `user_id` beside an entity accessor called `userId`.
pub fn lower_camel_case(label: &str) -> String {
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

/// The conventional table for an entity label, pluralized per `jdl-sol.md`
/// §9.7.
///
/// **The rule is the spec's, in full, because a partial one renames tables.**
/// Canonical emitted `create table task` where the legacy generator emits
/// `tasks`, so `jails model import` silently pointed an imported project at a
/// table its database does not have -- and every statement the compiler then
/// wrote was against that name (`audit.md` A2.6). The spec is explicit about
/// which irregulars and which invariants, so guessing is neither necessary nor
/// permitted: "there is no project-level override map".
///
/// **Pluralization applies to the final snake-case word.** `SupportPerson` is
/// `support_people`, not `support_persons` — the last word is what carries
/// number, and a compound whose head is irregular is the ordinary case rather
/// than an edge one.
///
/// The suffix rules read the whole string rather than the last word, which is
/// equivalent: the last word *is* the suffix.
pub fn plural_snake_case(label: &str) -> String {
    let base = snake_case(label);
    let (prefix, last) = match base.rfind('_') {
        Some(at) => base.split_at(at + 1),
        None => ("", base.as_str()),
    };
    if let Some(plural) = irregular_plural(last) {
        return format!("{prefix}{plural}");
    }
    if let Some(stem) = base.strip_suffix("fe") {
        return format!("{stem}ves");
    }
    // `ff` is not the `f -> ves` case: cliffs, not clives.
    if !base.ends_with("ff")
        && let Some(stem) = base.strip_suffix('f')
    {
        return format!("{stem}ves");
    }
    if base.ends_with("ss")
        || base.ends_with('x')
        || base.ends_with('z')
        || base.ends_with("ch")
        || base.ends_with("sh")
    {
        format!("{base}es")
    } else if base.ends_with('s') {
        // Already plural: a second `s` is worse than nothing.
        base
    } else if let Some(stem) = base.strip_suffix('y')
        && stem
            .chars()
            .next_back()
            .is_some_and(|before| !matches!(before, 'a' | 'e' | 'i' | 'o' | 'u'))
    {
        format!("{stem}ies")
    } else {
        format!("{base}s")
    }
}

/// §9.7's irregular map and invariant list, exactly.
///
/// Eight irregulars and ten invariants, and no more: an irregular jails
/// guesses at is a table name a reader has to discover from a migration.
fn irregular_plural(word: &str) -> Option<&'static str> {
    Some(match word {
        "person" => "people",
        "child" => "children",
        "man" => "men",
        "woman" => "women",
        "foot" => "feet",
        "tooth" => "teeth",
        "goose" => "geese",
        "mouse" => "mice",
        "equipment" => "equipment",
        "information" => "information",
        "money" => "money",
        "news" => "news",
        "series" => "series",
        "species" => "species",
        "staff" => "staff",
        "audio" => "audio",
        "metadata" => "metadata",
        "data" => "data",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::plural_snake_case;

    /// `jdl-sol.md` §9.7's rules, each spelled out.
    ///
    /// **Table-driven against the spec rather than against the other
    /// implementation**, because the other one is in `jails-protocol` -- a
    /// legacy crate this ladder cannot depend on. Until the cutover deletes
    /// it, two implementations of one rule is the situation, and the spec is
    /// what makes them one rule rather than two behaviours. The cross-check
    /// that they actually agree lives in `tests/`, which can see both.
    #[test]
    fn pluralization_follows_the_specified_rules() {
        for (label, expected) in [
            // The regular case, and the compound whose last word carries it.
            ("reward", "rewards"),
            ("work_item", "work_items"),
            // `ss|x|z|ch|sh -> ...es`
            ("address", "addresses"),
            ("box", "boxes"),
            ("quiz", "quizes"),
            ("batch", "batches"),
            ("dish", "dishes"),
            // consonant + `y -> ies`, but a vowel before it does not.
            ("category", "categories"),
            ("toy", "toys"),
            // `fe -> ves`, `f -> ves`, and `ff` is neither.
            ("knife", "knives"),
            ("shelf", "shelves"),
            ("cliff", "cliffs"),
            // An existing final `s` is left alone: a second one is worse
            // than nothing.
            ("status", "status"),
        ] {
            assert_eq!(plural_snake_case(label), expected, "{label}");
        }
    }

    /// The eight irregulars and ten invariants §9.7 names, and no others.
    ///
    /// The last word decides, so a compound whose head is irregular is the
    /// ordinary case rather than an edge one -- `support_persons` is the kind
    /// of name that reads as a bug in somebody else's schema.
    #[test]
    fn the_irregular_map_and_invariant_list_are_the_specified_ones() {
        for (label, expected) in [
            ("person", "people"),
            ("child", "children"),
            ("man", "men"),
            ("woman", "women"),
            ("foot", "feet"),
            ("tooth", "teeth"),
            ("goose", "geese"),
            ("mouse", "mice"),
            ("support_person", "support_people"),
            ("equipment", "equipment"),
            ("information", "information"),
            ("money", "money"),
            ("news", "news"),
            ("series", "series"),
            ("species", "species"),
            ("staff", "staff"),
            ("audio", "audio"),
            ("metadata", "metadata"),
            ("data", "data"),
        ] {
            assert_eq!(plural_snake_case(label), expected, "{label}");
        }
        // Not guessed at: an irregular jails invents is a table name a reader
        // has to discover from a migration.
        assert_eq!(plural_snake_case("ox"), "oxes");
    }
}
