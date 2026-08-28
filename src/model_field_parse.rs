//! Compact field syntax shared by canonical CLI frontends.

use super::ParsedField;
use crate::model_resource::java_to_label;
use jails_support::{Failure, Result};

pub(crate) fn parse_field(token: &str) -> Result<ParsedField> {
    let mut pieces = token.split('@');
    let shape = pieces.next().unwrap_or_default();
    let (name, mut type_name) = shape.split_once(':').ok_or_else(|| {
        Failure::Told(format!(
            "`{token}` is not a field declaration.\n       fix: use `name:type`, optionally followed by `!`, `?`, `@pk`, `@unique`, or `@index`"
        ))
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
        return Err(Failure::Told(format!(
            "`{token}` has an empty field name or type\n       fix: provide both sides of `name:type`"
        )));
    }
    let mut primary_key = false;
    let mut unique = false;
    let mut indexed = false;
    for marker in pieces {
        match marker {
            "pk" => primary_key = true,
            "unique" => unique = true,
            "index" => indexed = true,
            other => {
                return Err(Failure::Told(format!(
                    "`@{other}` is not represented by the canonical record model.\n       fix: use `@pk`, `@unique`, or `@index`"
                )));
            }
        }
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
    })
}

fn parse_length_shape<'a>(
    token: &str,
    type_name: &'a str,
) -> Result<(&'a str, Option<u32>, Option<u32>)> {
    let Some(open) = type_name.find('(') else {
        return Ok((type_name, None, None));
    };
    if !type_name.ends_with(')') {
        return Err(Failure::Told(format!(
            "`{token}` has an unclosed length range.\n       fix: write a range such as `string!(1..200)`"
        )));
    }
    let bounds = &type_name[open + 1..type_name.len() - 1];
    let (min, max) = bounds.split_once("..").ok_or_else(|| {
        Failure::Told(format!(
            "`{bounds}` is not a length range.\n       fix: use `min..max`, `min..`, or `..max`"
        ))
    })?;
    let parse = |value: &str| {
        if value.trim().is_empty() {
            Ok(None)
        } else {
            value.trim().parse::<u32>().map(Some).map_err(|_| {
                Failure::Told(format!(
                    "`{value}` is not a length bound.\n       fix: use a non-negative integer"
                ))
            })
        }
    };
    let min = parse(min)?;
    let max = parse(max)?;
    if min.is_none() && max.is_none() {
        return Err(Failure::Told(
            "a length range needs at least one bound.\n       fix: use `min..max`, `min..`, or `..max`"
                .to_string(),
        ));
    }
    Ok((&type_name[..open], min, max))
}

pub(crate) fn normalize_type(value: &str) -> String {
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
        "Currency" => "currency",
        other => other,
    }
    .to_string()
}
