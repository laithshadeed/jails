//! Canonical-constructor validation for generated Java records.

use jails_model::{BuiltinType, Field, TypeRef};
use std::collections::BTreeSet;

pub(super) fn record_checks(field: &Field, imports: &mut BTreeSet<String>) -> Vec<String> {
    let name = &field.names.java_member;
    if !field.required {
        imports.insert("java.util.Objects".to_string());
        let mut statements = vec![format!(
            "{name} = Objects.requireNonNullElse({name}, Optional.empty());"
        )];
        if let Some((condition, message)) = length_check(field, &format!("{name}.orElseThrow()")) {
            statements.push(illegal_argument(
                &format!("{name}.isPresent() && ({condition})"),
                &message,
            ));
        }
        if let Some((condition, message)) = numeric_check(field, &format!("{name}.orElseThrow()")) {
            statements.push(illegal_argument(
                &format!("{name}.isPresent() && ({condition})"),
                &message,
            ));
        }
        return statements;
    }
    let mut statements = Vec::new();
    if !super::primitive(field) {
        imports.insert("java.util.Objects".to_string());
        statements.push(format!("Objects.requireNonNull({name}, \"{name}\");"));
        if field.non_blank {
            statements.extend([
                format!("{name} = {name}.trim();"),
                illegal_argument(
                    &format!("{name}.isEmpty()"),
                    &format!("{name} must not be blank"),
                ),
            ]);
        }
    }
    if let Some((condition, message)) = length_check(field, name) {
        statements.push(illegal_argument(&condition, &message));
    }
    if let Some((condition, message)) = numeric_check(field, name) {
        statements.push(illegal_argument(&condition, &message));
    }
    statements
}

fn length_check(field: &Field, value: &str) -> Option<(String, String)> {
    let length = field.length.as_ref()?;
    let name = &field.names.java_member;
    Some(match (length.min, length.max) {
        (Some(min), Some(max)) => (
            format!("{value}.length() < {min} || {value}.length() > {max}"),
            format!("{name} length must be between {min} and {max}"),
        ),
        (Some(min), None) => (
            format!("{value}.length() < {min}"),
            format!("{name} length must be at least {min}"),
        ),
        (None, Some(max)) => (
            format!("{value}.length() > {max}"),
            format!("{name} length must be at most {max}"),
        ),
        (None, None) => unreachable!("linked length ranges have at least one bound"),
    })
}

fn numeric_check(field: &Field, value: &str) -> Option<(String, String)> {
    let (comparison, description) = if field.semantics.positive {
        ("<= 0", "positive")
    } else if field.semantics.nonnegative {
        ("< 0", "nonnegative")
    } else {
        return None;
    };
    let condition = match field.ty {
        TypeRef::Builtin(BuiltinType::Decimal) => {
            if field.semantics.positive {
                format!("{value}.signum() <= 0")
            } else {
                format!("{value}.signum() < 0")
            }
        }
        TypeRef::Builtin(BuiltinType::Integer | BuiltinType::Long | BuiltinType::Double) => {
            format!("{value} {comparison}")
        }
        _ => unreachable!("the linker accepts numeric constraints only on numeric fields"),
    };
    Some((
        condition,
        format!("{} must be {description}", field.names.java_member),
    ))
}

fn illegal_argument(condition: &str, message: &str) -> String {
    format!(
        "if ({condition}) {{\n            throw new IllegalArgumentException(\"{message}\");\n        }}"
    )
}
