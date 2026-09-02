//! Reading one attribute off a declaration.
//!
//! The walk above decides which declaration it is in; everything here answers
//! a question about a single `@name(...)` -- is it present, does it carry one
//! argument, is that argument a length, a scope, an escape sequence -- and
//! refuses by name when it is not. Kept apart because the refusals are the
//! bulk of it and none of them depends on where in the grammar the attribute
//! was found.

use super::*;

pub(super) fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    name: &str,
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if slot.replace(value).is_some() {
        return Err(parser.here(
            "JDL0211",
            format!("app property `{name}` is declared more than once"),
            format!("keep one `{name}` property"),
        ));
    }
    Ok(())
}

pub(super) fn reject_unknown_attributes(
    attributes: &[Attribute],
    allowed: &[&str],
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if let Some(attribute) = attributes
        .iter()
        .find(|attribute| !allowed.contains(&attribute.name.as_str()))
    {
        return Err(parser.here(
            "JDL0114",
            format!("attribute `@{}` is not valid here", attribute.name),
            format!("use only {}", allowed.join(", ")),
        ));
    }
    Ok(())
}

pub(super) fn one_arg(attributes: &[Attribute], name: &str) -> Result<Option<String>, Diagnostics> {
    one_attribute(attributes, name, |attribute| attribute.args[0].clone())
}

pub(super) fn one_raw_arg(
    attributes: &[Attribute],
    name: &str,
) -> Result<Option<String>, Diagnostics> {
    one_attribute(attributes, name, |attribute| attribute.raw_args[0].clone())
}

pub(super) fn one_attribute(
    attributes: &[Attribute],
    name: &str,
    value: impl FnOnce(&Attribute) -> String,
) -> Result<Option<String>, Diagnostics> {
    let matches = attributes
        .iter()
        .filter(|attribute| attribute.name == name)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` is repeated"),
            "keep one attribute of each kind",
        ));
    }
    let Some(attribute) = matches.first() else {
        return Ok(None);
    };
    if attribute.args.len() != 1 {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` needs exactly one argument"),
            format!("write `@{name}(value)`"),
        ));
    }
    Ok(Some(value(attribute)))
}

pub(super) fn has_attribute(attributes: &[Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| attribute.name == name)
}

pub(super) fn flag_attribute(attributes: &[Attribute], name: &str) -> Result<bool, Diagnostics> {
    let matches = attributes
        .iter()
        .filter(|attribute| attribute.name == name)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` is repeated"),
            "keep one attribute of each kind",
        ));
    }
    let Some(attribute) = matches.first() else {
        return Ok(false);
    };
    if attribute.parenthesized {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` does not accept arguments"),
            format!("write `@{name}`"),
        ));
    }
    Ok(true)
}

pub(super) fn field_scope(
    attributes: &[Attribute],
    parser: &Parser<'_>,
) -> Result<Option<source::FieldScope>, Diagnostics> {
    let matches = attributes
        .iter()
        .filter(|attribute| attribute.name == "scope")
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(parser.here(
            "JDL0513",
            "attribute `@scope` is repeated",
            "keep one scope attribute",
        ));
    }
    let Some(attribute) = matches.first() else {
        return Ok(None);
    };
    if !attribute.parenthesized {
        return Ok(Some(source::FieldScope { claim: None }));
    }
    if attribute.raw_args.len() != 1 {
        return Err(parser.here(
            "JDL0513",
            "attribute `@scope` accepts only one named claim argument",
            "write `@scope` or `@scope(claim: \"name\")`",
        ));
    }
    let raw = &attribute.raw_args[0];
    let Some(encoded) = raw.strip_prefix("claim:") else {
        return Err(parser.here(
            "JDL0513",
            "attribute `@scope` accepts only the named argument `claim`",
            "write `@scope(claim: \"name\")`",
        ));
    };
    let claim = serde_json::from_str::<String>(encoded).map_err(|_| {
        parser.here(
            "JDL0513",
            "the scope claim must be a quoted JSON string",
            "write `@scope(claim: \"name\")`",
        )
    })?;
    if claim.is_empty() {
        return Err(parser.here(
            "JDL0513",
            "the scope claim cannot be empty",
            "provide the authenticated claim name",
        ));
    }
    Ok(Some(source::FieldScope { claim: Some(claim) }))
}

pub(super) fn length(
    attributes: &[Attribute],
    parser: &Parser<'_>,
) -> Result<(Option<u32>, Option<u32>), Diagnostics> {
    let Some(value) = one_arg(attributes, "length")? else {
        return Ok((None, None));
    };
    let Some((min, max)) = value.split_once("..") else {
        return Err(parser.here(
            "JDL0512",
            format!("`{value}` is not a length range"),
            "use `@length(1..200)`, `@length(..200)`, or `@length(1..)`",
        ));
    };
    let bound = |value: &str| {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<u32>().map(Some).map_err(|_| {
                parser.here(
                    "JDL0512",
                    format!("`{value}` is not a non-negative length bound"),
                    "use an unsigned integer bound",
                )
            })
        }
    };
    let result = (bound(min)?, bound(max)?);
    if result == (None, None) {
        return Err(parser.here(
            "JDL0512",
            "a length range needs at least one bound",
            "provide a minimum, a maximum, or both",
        ));
    }
    Ok(result)
}

pub(super) fn decode_argument(value: &str) -> Result<String, Diagnostics> {
    if value.starts_with('"') {
        serde_json::from_str(value).map_err(|error| {
            Diagnostics::jdl_syntax(
                1,
                format!("invalid string argument: {error}"),
                "use a valid JSON-style string",
            )
        })
    } else {
        Ok(value.replace([' ', '\t', '\r', '\n'], ""))
    }
}

pub(super) fn stable_fragment(value: &str) -> String {
    crate::naming::stable_fragment(value)
}
