//! The compact field syntax: `name:type[!?]` and its `@` markers.
//!
//! **One parser, and it lives beside the alias table it answers to.**
//! `normalize_type` canonicalizes the CLI's Java spellings onto the builtin
//! names that `BuiltinType::from_alias` refuses a bare alias by, and
//! the case rule -- lowercase is jails' table, capitalised is a type the
//! project owns -- is decided here and nowhere else. A second parser of this
//! syntax is the repository's most reliable drift generator, which is why the
//! binary calls this one rather than keeping its own.
//!
//! A refusal is a `String`, the way every refusal in this crate is; the
//! binary wraps it in its own failure type at the call site.

pub fn parse_field(token: &str) -> Result<ParsedField, String> {
    let mut pieces = token.split('@');
    let shape = pieces.next().unwrap_or_default();
    let (name, mut type_name) = shape.split_once(':').ok_or_else(|| {
        format!(
            "`{token}` is not a field declaration.\n       fix: use `name:type`, optionally followed by `!`, `?`, `@pk`, `@unique`, or `@index`"
        )
    })?;
    let name = name.trim();
    type_name = type_name.trim();
    let (type_shape, min_length, max_length) = parse_length_shape(token, type_name)?;
    type_name = type_shape;
    let (required, non_blank) = if let Some(inner) = type_name.strip_suffix('!') {
        type_name = inner;
        (true, true)
    } else if let Some(inner) = type_name.strip_suffix('?') {
        type_name = inner;
        (false, false)
    } else {
        (true, false)
    };
    if name.is_empty() || type_name.is_empty() {
        return Err(format!(
            "`{token}` has an empty field name or type\n       fix: provide both sides of `name:type`"
        ));
    }
    let mut primary_key = false;
    let mut unique = false;
    let mut indexed = false;
    let mut positive = false;
    let mut nonnegative = false;
    let mut scoped = false;
    let mut version = false;
    let mut updated = false;
    let mut mapped_column = None;
    let mut default = None;
    for marker in pieces {
        match marker {
            "pk" => set_flag(token, marker, &mut primary_key)?,
            "unique" => set_flag(token, marker, &mut unique)?,
            "index" => set_flag(token, marker, &mut indexed)?,
            "positive" => set_flag(token, marker, &mut positive)?,
            "nonnegative" => set_flag(token, marker, &mut nonnegative)?,
            "scope" => set_flag(token, marker, &mut scoped)?,
            "version" => set_flag(token, marker, &mut version)?,
            "updated" => set_flag(token, marker, &mut updated)?,
            marker if argument(marker, "column").is_some() || argument(marker, "map").is_some() => {
                if mapped_column.is_some() {
                    return Err(format!(
                        "`{token}` names its physical column more than once.\n       fix: keep one `@column(name)` binding"
                    ));
                }
                let value = argument(marker, "column")
                    .or_else(|| argument(marker, "map"))
                    .expect("guarded marker argument");
                let value = decode_string_argument(value).map_err(|fix| {
                    format!("`@{marker}` has an invalid physical column.\n       fix: {fix}")
                })?;
                mapped_column = Some(value);
            }
            marker if argument(marker, "default").is_some() => {
                if default.is_some() {
                    return Err(format!(
                        "`{token}` repeats `@default`.\n       fix: keep one typed default expression"
                    ));
                }
                let value = argument(marker, "default").expect("guarded marker argument");
                if value.trim().is_empty() {
                    return Err(
                        "`@default()` has no value.\n       fix: provide a scalar, enum constant, uuid7(), identity(), now(), or today()"
                            .to_string(),
                    );
                }
                default = Some(value.trim().to_string());
            }
            other => {
                return Err(format!(
                    "`@{other}` is not represented by the canonical field model.\n       fix: use a documented field marker such as `@pk`, `@scope`, `@positive`, or `@column(name)`"
                ));
            }
        }
    }
    if positive && nonnegative {
        return Err(format!(
            "`{token}` is both positive and nonnegative.\n       fix: keep exactly one numeric constraint"
        ));
    }
    Ok(ParsedField {
        label: java_to_label(name),
        java_name: name.to_string(),
        type_name: normalize_type(type_name),
        required,
        non_blank,
        primary_key,
        unique,
        indexed,
        min_length,
        max_length,
        positive,
        nonnegative,
        scoped,
        version,
        default,
        updated,
        mapped_column,
    })
}

fn set_flag(token: &str, marker: &str, value: &mut bool) -> Result<(), String> {
    if std::mem::replace(value, true) {
        return Err(format!(
            "`{token}` repeats `@{marker}`.\n       fix: write each field marker once"
        ));
    }
    Ok(())
}

fn argument<'a>(marker: &'a str, name: &str) -> Option<&'a str> {
    marker
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
}

fn decode_string_argument(value: &str) -> Result<String, &'static str> {
    const EMPTY_COLUMN_FIX: &str = "provide a non-empty column name";
    let value = value.trim();
    if value.is_empty() {
        return Err(EMPTY_COLUMN_FIX);
    }
    if value.starts_with('"') {
        return serde_json::from_str::<String>(value)
            .map_err(|_| "use `@column(name)` or a valid JSON string in `@map(\"name\")`");
    }
    Ok(value.to_string())
}

fn parse_length_shape<'a>(
    token: &str,
    type_name: &'a str,
) -> Result<(&'a str, Option<u32>, Option<u32>), String> {
    let Some(open) = type_name.find('(') else {
        return Ok((type_name, None, None));
    };
    if !type_name.ends_with(')') {
        return Err(format!(
            "`{token}` has an unclosed length range.\n       fix: write a range such as `string!(1..200)`"
        ));
    }
    let bounds = &type_name[open + 1..type_name.len() - 1];
    let (min, max) = bounds.split_once("..").ok_or_else(|| {
        format!(
            "`{bounds}` is not a length range.\n       fix: use `min..max`, `min..`, or `..max`"
        )
    })?;
    let parse = |value: &str| {
        if value.trim().is_empty() {
            Ok(None)
        } else {
            value.trim().parse::<u32>().map(Some).map_err(|_| {
                format!("`{value}` is not a length bound.\n       fix: use a non-negative integer")
            })
        }
    };
    let min = parse(min)?;
    let max = parse(max)?;
    if min.is_none() && max.is_none() {
        return Err(
            "a length range needs at least one bound.\n       fix: use `min..max`, `min..`, or `..max`"
                .to_string(),
        );
    }
    Ok((&type_name[..open], min, max))
}

pub fn normalize_type(value: &str) -> String {
    match value {
        "text" | "String" => "string",
        "integer" | "Integer" => "int",
        "Long" => "long",
        "Double" => "double",
        "Boolean" => "boolean",
        "UUID" => "uuid",
        "LocalDate" => "date",
        "LocalDateTime" => "datetime",
        "timestamp" | "Instant" => "instant",
        "bigdecimal" | "BigDecimal" => "decimal",
        "Duration" => "duration",
        "URI" => "uri",
        "Path" => "path",
        "zoneid" | "ZoneId" => "zone-id",
        // **No `"Currency" => "currency"` row**, and its absence is the point.
        // `jails_spec::builtin_by_java_name` is the authority on which Java
        // spellings are builtins and deliberately omits this one, because an
        // enum of the currencies a project deals in is an ordinary thing to
        // generate -- so `currency:Currency` must mean the project's type. A
        // row here would be a second authority on the same question, and a
        // project declaring `enum Currency` would get a component of
        // `java.util.Currency`. The lowercase `currency` token is unaffected;
        // it is jails' table by the field syntax's own case rule.
        other => other,
    }
    .to_string()
}

/// One field, as the compact syntax spelled it.
pub struct ParsedField {
    pub label: String,
    pub java_name: String,
    pub type_name: String,
    pub required: bool,
    pub non_blank: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub positive: bool,
    pub nonnegative: bool,
    pub scoped: bool,
    pub version: bool,
    pub default: Option<String>,
    pub updated: bool,
    pub mapped_column: Option<String>,
}

pub fn java_to_label(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character == '-' {
            output.push('_');
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_fields_retain_every_jdl_v1_semantic_marker() {
        let field =
            parse_field("tenantId:uuid@scope@positive@column(tenant_id)@default(uuid7())@updated")
                .unwrap();
        assert!(field.scoped);
        assert!(field.positive);
        assert!(!field.nonnegative);
        assert_eq!(field.mapped_column.as_deref(), Some("tenant_id"));
        assert_eq!(field.default.as_deref(), Some("uuid7()"));
        assert!(field.updated);
    }

    #[test]
    fn compact_fields_refuse_repeated_and_contradictory_markers() {
        let repeated = parse_field("amount:int@positive@positive").err().unwrap();
        assert!(repeated.to_string().contains("repeats `@positive`"));
        let collision = parse_field("amount:int@positive@nonnegative")
            .err()
            .unwrap();
        assert!(
            collision
                .to_string()
                .contains("both positive and nonnegative")
        );
    }
}
