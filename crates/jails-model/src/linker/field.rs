//! Semantic checks for field constraints and compiler-managed behavior.

use super::Linker;
use crate::builtin::LiteralKind;
use crate::model::{BuiltinType, FieldDefault, FieldScope, FieldSemantics, LengthRange, TypeRef};
use crate::operation::Value;
use crate::source;
use std::collections::BTreeMap;

pub(super) fn constraints(
    non_blank: bool,
    min: Option<u32>,
    max: Option<u32>,
    required: bool,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> Option<LengthRange> {
    if non_blank && !matches!(ty, Some(TypeRef::Builtin(BuiltinType::String))) {
        linker.problem(
            "model-non-blank-type",
            format!("{path}.non_blank"),
            "`non_blank` is valid only for builtin `string` fields",
            "remove `non_blank` or change the field type to `string`",
        );
    }
    if non_blank && !required {
        linker.problem(
            "model-non-blank-required",
            format!("{path}.required"),
            "a non-blank field cannot be optional",
            "remove `?` or remove `@notBlank`",
        );
    }
    length_range(min, max, ty, path, linker)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn semantics(
    source: source::FieldSemantics,
    field_name: &str,
    required: bool,
    primary_key: bool,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> FieldSemantics {
    if source.positive || source.nonnegative {
        if !is_numeric(ty) {
            linker.problem(
                "model-numeric-constraint-type",
                format!("{path}.semantics"),
                "positive and nonnegative constraints require a numeric scalar field",
                "remove the numeric constraint or use int, long, double, or decimal",
            );
        }
        if source.positive && source.nonnegative {
            linker.problem(
                "model-numeric-constraint-collision",
                format!("{path}.semantics"),
                "a field cannot be both positive and nonnegative",
                "keep the one constraint that describes the accepted range",
            );
        }
    }

    let scope = source.scope.map(|scope| {
        if !required {
            linker.problem(
                "model-scope-required",
                format!("{path}.required"),
                "a scope field cannot be optional",
                "remove `?` from the scope field",
            );
        }
        if !matches!(ty, Some(TypeRef::Builtin(builtin)) if builtin.semantics().scopeable) {
            linker.problem(
                "model-scope-type",
                format!("{path}.type"),
                "a scope field must be string, uuid, int, or long",
                "change the type or remove `@scope`",
            );
        }
        let pinned = scope.claim.is_some();
        FieldScope {
            claim: scope.claim.unwrap_or_else(|| field_name.to_string()),
            pinned,
        }
    });

    if source.version {
        if !required || !matches!(ty, Some(TypeRef::Builtin(BuiltinType::Long))) {
            linker.problem(
                "model-version-type",
                format!("{path}.semantics.version"),
                "a version field must be required builtin `long`",
                "use `long @version`",
            );
        }
        if scope.is_some() {
            linker.problem(
                "model-scope-version",
                format!("{path}.semantics"),
                "a scope field cannot also be the optimistic-lock version",
                "put `@scope` and `@version` on different fields",
            );
        }
        if source.positive {
            linker.problem(
                "model-version-positive",
                format!("{path}.semantics.positive"),
                "a version field starts at zero and cannot be positive-only",
                "use `@nonnegative` on a version field",
            );
        }
    }

    if source.updated
        && (!required
            || !matches!(
                ty,
                Some(TypeRef::Builtin(
                    BuiltinType::Instant | BuiltinType::DateTime
                ))
            ))
    {
        linker.problem(
            "model-updated-type",
            format!("{path}.semantics.updated"),
            "an updated field must be required builtin `instant` or `datetime`",
            "use `instant @updated` or `datetime @updated`",
        );
    }

    let explicit_default = source.default.is_some();
    if scope.is_some() && explicit_default {
        linker.problem(
            "model-scope-default",
            format!("{path}.semantics.default"),
            "a scope field is supplied by execution context and cannot have a default",
            "remove `@default` from the scope field",
        );
    }
    if source.version && explicit_default {
        linker.problem(
            "model-version-default",
            format!("{path}.semantics.default"),
            "a version field has compiler-managed initial value zero",
            "remove `@default` from the version field",
        );
    }

    let default = source
        .default
        .map(|value| FieldDefault {
            value: link_default(value, primary_key, ty, path, linker),
            derived: false,
        })
        .or_else(|| derive_default(primary_key, source.version, ty));

    FieldSemantics {
        positive: source.positive,
        nonnegative: source.nonnegative,
        scope,
        version: source.version,
        default,
        updated: source.updated,
    }
}

pub(super) fn validate_scope_claims(
    entity_path: &str,
    fields: &[crate::Field],
    linker: &mut Linker,
) {
    let mut claims = BTreeMap::<&str, &str>::new();
    let mut version = None::<&str>;
    for field in fields.iter() {
        if field.semantics.version
            && let Some(first) = version.replace(&field.label)
        {
            linker.problem(
                "model-version-count",
                format!("{entity_path}.fields.{}.semantics.version", field.label),
                format!("version field `{}` conflicts with `{first}`", field.label),
                "keep exactly one optimistic-lock version field per entity",
            );
        }
        let Some(scope) = &field.semantics.scope else {
            continue;
        };
        if let Some(first) = claims.insert(&scope.claim, &field.label) {
            linker.problem(
                "model-scope-claim-collision",
                format!("{entity_path}.fields.{}.semantics.scope", field.label),
                format!(
                    "scope claim `{}` is already used by field `{first}`",
                    scope.claim
                ),
                "give each scope field a unique authenticated claim name",
            );
        }
    }
}

fn is_numeric(ty: Option<&TypeRef>) -> bool {
    matches!(ty, Some(TypeRef::Builtin(builtin)) if builtin.semantics().numeric)
}

fn derive_default(primary_key: bool, version: bool, ty: Option<&TypeRef>) -> Option<FieldDefault> {
    let value = if version {
        Value::Integer("0".to_string())
    } else if primary_key && matches!(ty, Some(TypeRef::Builtin(BuiltinType::Uuid))) {
        function("uuid7")
    } else if primary_key
        && matches!(
            ty,
            Some(TypeRef::Builtin(BuiltinType::Integer | BuiltinType::Long))
        )
    {
        function("identity")
    } else {
        return None;
    };
    Some(FieldDefault {
        value,
        derived: true,
    })
}

fn function(name: &str) -> Value {
    Value::Function {
        name: name.to_string(),
        arguments: Vec::new(),
    }
}

fn link_default(
    value: source::Value,
    primary_key: bool,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> Value {
    let value = link_value(value);
    // Every arm asks the builtin's own row rather than a match of its own: a
    // negated string arm accepts a builtin added to the enum by default,
    // rather than refusing until somebody says what it accepts.
    let compatible = match (&value, ty) {
        (Value::String(_), Some(TypeRef::Builtin(builtin))) => {
            builtin.semantics().literal == LiteralKind::Text
        }
        (Value::Integer(raw), Some(TypeRef::Builtin(builtin))) => {
            match builtin.semantics().literal {
                LiteralKind::Int32 => raw.parse::<i32>().is_ok(),
                LiteralKind::Int64 => raw.parse::<i64>().is_ok(),
                LiteralKind::Fractional => true,
                _ => false,
            }
        }
        (Value::Decimal(_), Some(TypeRef::Builtin(builtin))) => {
            builtin.semantics().literal == LiteralKind::Fractional
        }
        (Value::Boolean(_), Some(TypeRef::Builtin(builtin))) => {
            builtin.semantics().literal == LiteralKind::Boolean
        }
        (Value::EnumConstant(_), Some(TypeRef::External(_))) => true,
        (Value::Function { name, arguments }, Some(TypeRef::Builtin(builtin)))
            if arguments.is_empty() =>
        {
            builtin.semantics().defaults.contains(&name.as_str())
                // `identity` is the one whose validity is about the field
                // rather than the type: an auto-assigned value that is not
                // the key is a number nothing assigns.
                && (name != "identity" || primary_key)
        }
        _ => false,
    };
    if !compatible {
        linker.problem(
            "model-field-default-type",
            format!("{path}.semantics.default"),
            "the field default is not a registered expression for this field type",
            "use a matching literal, uuid7(), identity(), now(), or today()",
        );
    }
    value
}

fn link_value(value: source::Value) -> Value {
    match value {
        source::Value::String(value) => Value::String(value),
        source::Value::Integer(value) => Value::Integer(value),
        source::Value::Decimal(value) => Value::Decimal(value),
        source::Value::Boolean(value) => Value::Boolean(value),
        source::Value::EnumConstant(value) => Value::EnumConstant(value),
        source::Value::Function(call) => Value::Function {
            name: call.name,
            arguments: call.arguments.into_iter().map(link_value).collect(),
        },
    }
}

fn length_range(
    min: Option<u32>,
    max: Option<u32>,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> Option<LengthRange> {
    if min.is_none() && max.is_none() {
        return None;
    }
    if !matches!(ty, Some(TypeRef::Builtin(BuiltinType::String))) {
        linker.problem(
            "model-length-type",
            format!("{path}.min_length"),
            "length bounds are valid only for builtin `string` fields",
            "remove the bounds or change the field type to `string`",
        );
    }
    if matches!((min, max), (Some(min), Some(max)) if min > max) {
        linker.problem(
            "model-length-order",
            format!("{path}.min_length"),
            "the minimum length is greater than the maximum",
            "choose bounds where `min_length <= max_length`",
        );
    }
    Some(LengthRange { min, max })
}
