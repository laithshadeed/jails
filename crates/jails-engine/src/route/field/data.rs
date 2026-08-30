//! What a new column is filled with: a default, a backfill file, or neither.
//!
//! **The two are mutually exclusive and saying both is an error.** A default
//! and a backfill are two answers to one question, and picking either silently
//! would mean the column ends up holding what the *other* one said. There is
//! no last-one-wins here.
//!
//! A required column's backfill is reader-owned bytes captured as a
//! precondition and embedded verbatim ahead of `set not null`, so the plan
//! carries the exact statement that was reviewed rather than a path that might
//! read differently by the time it runs.

use super::*;
use jails_protocol::request::TypedLiteral;

pub(super) fn add_data_plan(
    project: &Project,
    field: &FieldSpec,
    table: &str,
    column: &str,
    default_literal: Option<&str>,
    backfill_file: Option<&str>,
) -> Result<(DataEvolution, Option<String>, Option<ProjectPath>)> {
    if default_literal.is_some() && backfill_file.is_some() {
        return Err("choose exactly one data plan.\n       fix: remove either `--default-literal` or `--backfill-file`.".into());
    }
    let required = field.optionality != Optionality::Nullable;
    if !required && (default_literal.is_some() || backfill_file.is_some()) {
        return Err("a nullable field does not need a backfill before it is introduced.\n       fix: remove the data-plan option, or declare the field as required.".into());
    }
    if !required {
        return Ok((DataEvolution::None, None, None));
    }

    if let Some(value) = default_literal {
        let typed = TypedLiteral::parse(value)?;
        let literal = typed_sql_literal(field, typed.as_str())?;
        let sql = format!("update {table}\n  set {column} = {literal}\n  where {column} is null;");
        return Ok((DataEvolution::TypedLiteral(typed), Some(sql), None));
    }
    if let Some(path) = backfill_file {
        let path = ProjectPath::parse(path)?;
        let sql = read_backfill(project, &path, column)?;
        return Ok((
            DataEvolution::ReaderOwnedSql(path.clone()),
            Some(sql),
            Some(path),
        ));
    }
    Err(format!(
        "required field `{}` needs a data plan for existing rows.\n       fix: pass \
         `--default-literal <typed-value>` or `--backfill-file <project-path>`.",
        field.name
    )
    .into())
}

pub(super) fn read_backfill(project: &Project, path: &ProjectPath, column: &str) -> Result<String> {
    let full = project.root().join(path.as_str());
    let sql = std::fs::read_to_string(&full).map_err(|error| {
        format!(
            "could not read backfill `{path}`: {error}.\n       fix: provide an existing UTF-8 \
             SQL file inside this project."
        )
    })?;
    if sql.trim().is_empty() {
        return Err(format!(
            "backfill `{path}` is empty.\n       fix: provide SQL that populates `{column}` \
             before it becomes required."
        )
        .into());
    }
    Ok(sql)
}

fn typed_sql_literal(field: &FieldSpec, value: &str) -> Result<String> {
    let kind = field.field_type.canonical();
    let invalid = || {
        format!(
            "`{value}` is not a valid `{kind}` default for `{}`.\n       fix: pass a lexical \
             value of that declared field type.",
            field.name
        )
    };
    Ok(match kind.as_str() {
        "int" => value.parse::<i32>().map_err(|_| invalid())?.to_string(),
        "long" => value.parse::<i64>().map_err(|_| invalid())?.to_string(),
        "decimal" | "double" | "currency" => {
            let parsed = value.parse::<f64>().map_err(|_| invalid())?;
            if !parsed.is_finite() {
                return Err(invalid().into());
            }
            value.to_string()
        }
        "boolean" if matches!(value, "true" | "false") => value.to_string(),
        "boolean" => return Err(invalid().into()),
        "uuid" if valid_uuid_literal(value) => format!("'{}'::uuid", value.to_ascii_lowercase()),
        "uuid" => return Err(invalid().into()),
        "string" => {
            if field.optionality == Optionality::NonBlank && value.trim().is_empty() {
                return Err(invalid().into());
            }
            format!("'{}'", value.replace('\'', "''"))
        }
        other if !other.starts_with("list<") && !other.starts_with("map<") => {
            format!("'{}'", value.replace('\'', "''"))
        }
        _ => {
            return Err(format!(
                "field `{}` has collection type `{kind}`, which has no one-column typed default.\n       \
                 fix: use `--backfill-file` with an explicit data migration.",
                field.name
            )
            .into());
        }
    })
}

fn valid_uuid_literal(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        })
}
