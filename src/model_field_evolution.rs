//! Canonical, typed field evolution frontends.

#[path = "model_jdl_edit.rs"]
mod jdl_edit;

use crate::Invocation;
use crate::cli::{ColumnRenamePolicy as CliColumnPolicy, TypeChangeStrategy as CliTypeStrategy};
use crate::model_generate::{PreparedMutation, finish_generation, normalize_type};
use jails_contracts::ProjectPath;
use jails_model::field_syntax::java_to_label;
use jails_model::{
    ColumnRenamePolicy, Evolution, EvolutionStep, FieldEvolutionPolicy, FieldId, StableId,
    TypeChangeStrategy,
};
use jails_support::{Failure, Result};

pub(crate) struct RenameRequest {
    pub(crate) entity: String,
    pub(crate) field: String,
    pub(crate) new_name: String,
    pub(crate) column: CliColumnPolicy,
    pub(crate) package: Option<String>,
}

pub(crate) fn rename(request: RenameRequest, invocation: Invocation) -> Result<()> {
    reject_package(request.package.as_deref())?;
    let resolved = resolve(&request.entity, &request.field, &invocation)?;
    let (column, mut next_source) = match request.column {
        CliColumnPolicy::Rolling => {
            return Err(Failure::Told(
                "rolling column rename is a multi-release campaign, not one patch.\n       fix: use `--column preserve` or `--column single-cutover`"
                    .to_string(),
            ));
        }
        CliColumnPolicy::Preserve => (
            ColumnRenamePolicy::Preserve,
            rename_source_field(
                &resolved,
                &request.new_name,
                Some(resolved.field_sql_column.as_str()),
            )?,
        ),
        CliColumnPolicy::SingleCutover => (
            ColumnRenamePolicy::SingleCutover,
            rename_source_field(
                &resolved,
                &request.new_name,
                Some(&java_to_label(&request.new_name)),
            )?,
        ),
    };
    finish_replace(
        resolved,
        &request.new_name,
        &mut next_source,
        FieldEvolutionPolicy::Rename { column },
        invocation,
        &[],
    )
}

pub(crate) struct TypeRequest {
    pub(crate) entity: String,
    pub(crate) field: String,
    pub(crate) to: String,
    pub(crate) strategy: CliTypeStrategy,
    pub(crate) package: Option<String>,
}

pub(crate) fn change_type(request: TypeRequest, invocation: Invocation) -> Result<()> {
    reject_package(request.package.as_deref())?;
    if request.strategy == CliTypeStrategy::ExpandContract {
        return Err(Failure::Told(
            "expand/contract is a multi-release campaign, not one patch.\n       fix: use `--strategy safe` for a proven widening"
                .to_string(),
        ));
    }
    let resolved = resolve(&request.entity, &request.field, &invocation)?;
    let to = normalize_type(&request.to);
    let mut next_source = jdl_edit::set_field_type(
        &resolved.current.source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
        &to,
    )?;
    finish_replace(
        resolved,
        &request.field,
        &mut next_source,
        FieldEvolutionPolicy::ChangeType {
            strategy: TypeChangeStrategy::Safe,
        },
        invocation,
        &[],
    )
}

pub(crate) struct NullabilityRequest {
    pub(crate) entity: String,
    pub(crate) field: String,
    pub(crate) nullable: bool,
    pub(crate) required: bool,
    pub(crate) backfill_file: Option<String>,
    pub(crate) package: Option<String>,
}

pub(crate) fn set_nullability(request: NullabilityRequest, invocation: Invocation) -> Result<()> {
    reject_package(request.package.as_deref())?;
    if request.nullable == request.required {
        return Err(Failure::Told(
            "choose exactly one of `--nullable` or `--required`.\n       fix: pass one nullability flag"
                .to_string(),
        ));
    }
    if request.nullable && request.backfill_file.is_some() {
        return Err(Failure::Told(
            "making a field nullable does not need a backfill.\n       fix: remove `--backfill-file`"
                .to_string(),
        ));
    }
    let resolved = resolve(&request.entity, &request.field, &invocation)?;
    let (backfill_sql, reader_paths) = match request.backfill_file.as_deref() {
        Some(path) => {
            let (path, bytes) = read_reader_sql(path)?;
            (Some(bytes), vec![path])
        }
        None if request.required && resolved.has_database => {
            return Err(Failure::Told(format!(
                "making `{}.{}` required needs an explicit backfill.\n       fix: pass `--backfill-file <project-path>`",
                resolved.entity_label, resolved.field_label
            )));
        }
        None => (None, Vec::new()),
    };
    let mut next_source = jdl_edit::set_field_required(
        &resolved.current.source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
        request.required,
    )?;
    finish_replace(
        resolved,
        &request.field,
        &mut next_source,
        FieldEvolutionPolicy::SetNullability {
            backfill_sql: backfill_sql.clone(),
        },
        invocation,
        &reader_paths,
    )
}

pub(crate) struct DropRequest {
    pub(crate) entity: String,
    pub(crate) field: String,
    pub(crate) confirm_column: String,
    pub(crate) package: Option<String>,
}

pub(crate) fn drop_field(request: DropRequest, invocation: Invocation) -> Result<()> {
    reject_package(request.package.as_deref())?;
    let resolved = resolve(&request.entity, &request.field, &invocation)?;
    resolved
        .current
        .model
        .refuse_field_removal(&resolved.field_id)
        .map_err(Failure::Told)?;
    let next_source = jdl_edit::remove_field(
        &resolved.current.source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
    )?;
    let evolution = Evolution::one(EvolutionStep::RemoveField {
        field: resolved.field_id.clone(),
        confirmed_column: request.confirm_column.clone(),
    });
    finish_generation(PreparedMutation {
        name: format!("{}.{}", request.entity, request.field),
        invocation,
        current: resolved.current,
        next_source,
        evolution,
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

struct ResolvedField {
    current: crate::model_command::Current,
    entity_label: String,
    entity_java_name: String,
    field_id: FieldId,
    field_label: String,
    field_java_name: String,
    field_sql_column: String,
    has_database: bool,
}

fn resolve(entity_name: &str, field_name: &str, invocation: &Invocation) -> Result<ResolvedField> {
    let current = crate::model_command::Current::load(invocation)?;
    let entity_label = java_to_label(entity_name);
    let entity = current.model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "entity `{entity_name}` does not exist.\n       fix: name an entity `.jails/model.jdl` declares"
            ))
        })?;
    let requested_field = java_to_label(field_name);
    let field = entity
        .fields
        .iter()
        .find(|field| field.label == requested_field || field.names.java_member == field_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "field `{entity_name}.{field_name}` does not exist.\n       fix: name a field `entity {}` declares in `.jails/model.jdl`",
                entity.label
            ))
        })?;
    entity.refuse_retired().map_err(Failure::Told)?;
    let resolved = ResolvedField {
        entity_label: entity.label.clone(),
        entity_java_name: entity.names.java_type.clone(),
        field_id: field.id.clone(),
        field_label: field.label.clone(),
        field_java_name: field.names.java_member.clone(),
        field_sql_column: field.names.sql_column.clone(),
        has_database: current
            .model
            .capabilities
            .values()
            .any(|capability| capability.kind == "db"),
        current,
    };
    Ok(resolved)
}

fn finish_replace(
    resolved: ResolvedField,
    display_name: &str,
    next_source: &mut String,
    policy: FieldEvolutionPolicy,
    invocation: Invocation,
    reader_paths: &[ProjectPath],
) -> Result<()> {
    let evolution = Evolution::one(EvolutionStep::ReplaceField {
        field: resolved.field_id.clone(),
        policy,
    });
    finish_generation(PreparedMutation {
        name: format!("{}.{}", resolved.entity_label, display_name),
        invocation,
        current: resolved.current,
        next_source: std::mem::take(next_source),
        evolution,
        authored_migration: None,
        reader_paths: reader_paths.to_vec(),
    })
}

fn reject_package(package: Option<&str>) -> Result<()> {
    if package.is_some() {
        return Err(Failure::Told(
            "entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    Ok(())
}

fn rename_source_field(
    resolved: &ResolvedField,
    next_name: &str,
    next_column: Option<&str>,
) -> Result<String> {
    jdl_edit::rename_field(
        &resolved.current.source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
        next_name,
        next_column,
    )
}

pub(crate) fn read_reader_sql(value: &str) -> Result<(ProjectPath, Vec<u8>)> {
    let path = ProjectPath::parse(value.to_string()).map_err(Failure::Told)?;
    let bytes = std::fs::read(path.as_str()).map_err(|error| {
        Failure::Told(format!(
            "could not read reader-owned SQL `{path}`: {error}\n       fix: pass an existing project-relative file"
        ))
    })?;
    let sql = std::str::from_utf8(&bytes).map_err(|_| {
        Failure::Told(format!(
            "reader-owned SQL `{path}` is not UTF-8.\n       fix: save the backfill as UTF-8 text"
        ))
    })?;
    if sql.trim().is_empty() {
        return Err(Failure::Told(format!(
            "reader-owned SQL `{path}` is empty.\n       fix: provide the data update that makes the constraint safe"
        )));
    }
    Ok((path, bytes))
}
