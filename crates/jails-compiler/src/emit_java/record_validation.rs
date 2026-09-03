//! Canonical-constructor validation for generated Java records.

use super::RecordComponent;
use jails_model::TypeRef;
use std::collections::BTreeSet;

pub(super) fn record_checks(
    component: &RecordComponent<'_>,
    imports: &mut BTreeSet<String>,
) -> Vec<String> {
    let name = component.name.as_str();
    if !component.required {
        imports.insert("java.util.Objects".to_string());
        let mut statements = vec![format!(
            "{name} = Objects.requireNonNullElse({name}, Optional.empty());"
        )];
        if let Some((condition, message)) =
            length_check(component, &format!("{name}.orElseThrow()"))
        {
            statements.push(illegal_argument(
                &format!("{name}.isPresent() && ({condition})"),
                &message,
            ));
        }
        if let Some((condition, message)) =
            numeric_check(component, &format!("{name}.orElseThrow()"))
        {
            statements.push(illegal_argument(
                &format!("{name}.isPresent() && ({condition})"),
                &message,
            ));
        }
        return statements;
    }
    let mut statements = Vec::new();
    if !super::primitive(component.ty, component.required) {
        imports.insert("java.util.Objects".to_string());
        statements.push(format!("Objects.requireNonNull({name}, \"{name}\");"));
        if component.non_blank {
            statements.extend([
                format!("{name} = {name}.trim();"),
                illegal_argument(
                    &format!("{name}.isEmpty()"),
                    &format!("{name} must not be blank"),
                ),
            ]);
        }
    }
    // **A defensive copy, because a record component is not private.** The
    // caller keeps their reference to the list they passed; `List.copyOf`
    // makes the record's own view immutable and unshared, which is what JDL
    // v1 §9.2 promises of a required collection. It rejects a null element
    // too, so the `requireNonNull` above and this together say the whole of
    // "this component is there and so is everything in it".
    match component.ty {
        TypeRef::List(_) => {
            imports.insert("java.util.List".to_string());
            statements.push(format!("{name} = List.copyOf({name});"));
        }
        TypeRef::Map(..) => {
            imports.insert("java.util.Map".to_string());
            statements.push(format!("{name} = Map.copyOf({name});"));
        }
        TypeRef::Builtin(_) | TypeRef::External(_) => {}
    }
    if let Some((condition, message)) = length_check(component, name) {
        statements.push(illegal_argument(&condition, &message));
    }
    if let Some((condition, message)) = numeric_check(component, name) {
        statements.push(illegal_argument(&condition, &message));
    }
    statements
}

fn length_check(component: &RecordComponent<'_>, value: &str) -> Option<(String, String)> {
    let length = component.length?;
    let name = component.name.as_str();
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

fn numeric_check(component: &RecordComponent<'_>, value: &str) -> Option<(String, String)> {
    let (comparison, description) = if component.positive {
        ("<= 0", "positive")
    } else if component.nonnegative {
        ("< 0", "nonnegative")
    } else {
        return None;
    };
    let condition = match component.ty {
        // A boxed number is compared through `signum()`, a primitive one with
        // an operator -- so the question is whether the builtin has a
        // primitive spelling, which is on its row.
        TypeRef::Builtin(builtin)
            if builtin.semantics().numeric && builtin.semantics().java_primitive.is_none() =>
        {
            if component.positive {
                format!("{value}.signum() <= 0")
            } else {
                format!("{value}.signum() < 0")
            }
        }
        TypeRef::Builtin(builtin) if builtin.semantics().numeric => {
            format!("{value} {comparison}")
        }
        _ => unreachable!("the linker accepts numeric constraints only on numeric fields"),
    };
    Some((
        condition,
        format!("{} must be {description}", component.name),
    ))
}

fn illegal_argument(condition: &str, message: &str) -> String {
    format!(
        "if ({condition}) {{\n            throw new IllegalArgumentException(\"{message}\");\n        }}"
    )
}
