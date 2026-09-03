//! PostgreSQL lowering for validated field constraints and typed defaults.

use crate::Diagnostic;
use jails_model::{Field, Value};

pub(super) enum SqlDefault {
    Application,
    Identity,
    Expression(String),
}

pub(super) fn initial_column(
    field: &Field,
    name: &str,
    sql_type: &str,
) -> Result<String, Diagnostic> {
    let mut column = format!("    {name} {sql_type}");
    match sql_default(field)? {
        Some(SqlDefault::Identity) => column.push_str(" generated always as identity"),
        Some(SqlDefault::Expression(value)) => column.push_str(&format!(" default {value}")),
        Some(SqlDefault::Application) | None => {}
    }
    if field.required {
        column.push_str(" not null");
    }
    if field.primary_key {
        column.push_str(" primary key");
    } else if field.unique {
        column.push_str(" unique");
    }
    if field.non_blank {
        column.push_str(&format!(" check (length(btrim({name})) > 0)"));
    }
    if let Some(check) = length_check(field) {
        column.push_str(&format!(" check ({check})"));
    }
    if let Some(check) = numeric_check(field) {
        column.push_str(&format!(" check ({check})"));
    }
    Ok(column)
}

pub(super) fn length_check(field: &Field) -> Option<String> {
    let range = field.length.as_ref()?;
    let column = &field.names.sql_column;
    Some(match (range.min, range.max) {
        (Some(min), Some(max)) => {
            format!("char_length({column}) between {min} and {max}")
        }
        (Some(min), None) => format!("char_length({column}) >= {min}"),
        (None, Some(max)) => format!("char_length({column}) <= {max}"),
        (None, None) => unreachable!("linked length ranges have at least one bound"),
    })
}

pub(super) fn numeric_check(field: &Field) -> Option<String> {
    let column = &field.names.sql_column;
    if field.semantics.positive {
        Some(format!("{column} > 0"))
    } else if field.semantics.nonnegative {
        Some(format!("{column} >= 0"))
    } else {
        None
    }
}

pub(super) fn sql_default(field: &Field) -> Result<Option<SqlDefault>, Diagnostic> {
    let Some(default) = &field.semantics.default else {
        return Ok(None);
    };
    let value = match &default.value {
        Value::String(value) => SqlDefault::Expression(quoted_sql(value)),
        Value::Integer(value) | Value::Decimal(value) => SqlDefault::Expression(value.clone()),
        Value::Boolean(value) => SqlDefault::Expression(value.to_string()),
        Value::EnumConstant(value) => SqlDefault::Expression(quoted_sql(value)),
        Value::Function { name, arguments } if arguments.is_empty() => match name.as_str() {
            "uuid7" => SqlDefault::Application,
            "identity" => SqlDefault::Identity,
            "now" => SqlDefault::Expression("current_timestamp".to_string()),
            "today" => SqlDefault::Expression("current_date".to_string()),
            other => {
                return Err(Diagnostic::new(
                    "compile-default-function-unknown",
                    field.id.to_string(),
                    format!(
                        "linked field `{}` carries unknown default function `{other}`",
                        field.label
                    ),
                    "re-link the source through the closed JDL default registry",
                ));
            }
        },
        Value::Function { name, .. } => {
            return Err(Diagnostic::new(
                "compile-default-function-arguments",
                field.id.to_string(),
                format!(
                    "linked field `{}` carries arguments for zero-argument default `{name}`",
                    field.label
                ),
                "remove the arguments and re-link the source",
            ));
        }
    };
    Ok(Some(value))
}

fn quoted_sql(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
