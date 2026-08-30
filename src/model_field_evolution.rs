//! Canonical, typed field evolution frontends.

#[path = "model_jdl_edit.rs"]
mod jdl_edit;

use crate::Invocation;
use crate::cli::{ColumnRenamePolicy as CliColumnPolicy, TypeChangeStrategy as CliTypeStrategy};
use crate::model_generate::{
    PreparedMutation, finish_generation, finish_generation_with_reader_paths, normalize_type,
};
use crate::model_resource::java_to_label;
use jails_contracts::ProjectPath;
use jails_model::{
    AppModel, ColumnRenamePolicy, EntityId, Field, FieldEvolutionPolicy, FieldId, ModelPatch,
    StableId, TypeChangeStrategy,
};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(crate) struct RenameRequest {
    pub(crate) entity: String,
    pub(crate) field: String,
    pub(crate) new_name: String,
    pub(crate) column: CliColumnPolicy,
    pub(crate) package: Option<String>,
}

pub(crate) fn rename(request: RenameRequest, invocation: Invocation) -> Result<()> {
    reject_package(request.package.as_deref())?;
    let resolved = resolve(&request.entity, &request.field)?;
    let (column, mut next_source) = match request.column {
        CliColumnPolicy::Rolling => {
            return Err(Failure::Told(
                "rolling column rename is a multi-release campaign, not one exact patch.\n       fix: use `--column preserve` or `--column single-cutover`"
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
        json!({
            "kind": "rename-field",
            "new_name": request.new_name,
            "column": match column {
                ColumnRenamePolicy::Preserve => "preserve",
                ColumnRenamePolicy::SingleCutover => "single-cutover",
            },
        }),
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
            "expand/contract is a multi-release campaign, not one exact patch.\n       fix: use `--strategy safe` for a proven widening"
                .to_string(),
        ));
    }
    let resolved = resolve(&request.entity, &request.field)?;
    let to = normalize_type(&request.to);
    let mut next_source = jdl_edit::set_field_type(
        &resolved.current_source,
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
        json!({"kind": "change-field-type", "to": to, "strategy": "safe"}),
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
    let resolved = resolve(&request.entity, &request.field)?;
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
        &resolved.current_source,
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
        json!({
            "kind": "set-field-nullability",
            "required": request.required,
            "backfill_sql": backfill_sql,
        }),
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
    let resolved = resolve(&request.entity, &request.field)?;
    let next_source = jdl_edit::remove_field(
        &resolved.current_source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
    )?;
    let patch = ModelPatch::RemoveField {
        entity: resolved.entity_id.clone(),
        field: resolved.field_id.clone(),
        confirmed_column: request.confirm_column.clone(),
    };
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "drop-field",
        "entity": resolved.entity_id,
        "field": resolved.field_id,
        "confirmed_column": request.confirm_column,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: format!("{}.{}", request.entity, request.field),
        invocation,
        model_path: resolved.model_path,
        current_source: resolved.current_source,
        current_model: resolved.current_model,
        next_source,
        patch,
        patch_bytes,
        authored_migration: None,
    })
}

struct ResolvedField {
    model_path: PathBuf,
    current_source: String,
    current_model: AppModel,
    entity_id: EntityId,
    entity_label: String,
    entity_java_name: String,
    field_id: FieldId,
    field_label: String,
    field_java_name: String,
    field_sql_column: String,
    has_database: bool,
}

fn resolve(entity_name: &str, field_name: &str) -> Result<ResolvedField> {
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source)?;
    let entity_label = java_to_label(entity_name);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{entity_name}` does not exist.\n       fix: name an entity declared under `[entities]`"
            ))
        })?;
    let requested_field = java_to_label(field_name);
    let field = entity
        .fields
        .iter()
        .find(|field| field.label == requested_field || field.names.java_member == field_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical field `{entity_name}.{field_name}` does not exist.\n       fix: name a field declared under `[entities.{}.fields]`",
                entity.label
            ))
        })?;
    let resolved = ResolvedField {
        model_path,
        current_source,
        entity_id: entity.id.clone(),
        entity_label: entity.label.clone(),
        entity_java_name: entity.names.java_type.clone(),
        field_id: field.id.clone(),
        field_label: field.label.clone(),
        field_java_name: field.names.java_member.clone(),
        field_sql_column: field.names.sql_column.clone(),
        has_database: current_model
            .capabilities
            .values()
            .any(|capability| capability.kind == "db"),
        current_model,
    };
    Ok(resolved)
}

fn finish_replace(
    resolved: ResolvedField,
    display_name: &str,
    next_source: &mut String,
    policy: FieldEvolutionPolicy,
    patch_json: serde_json::Value,
    invocation: Invocation,
    reader_paths: &[ProjectPath],
) -> Result<()> {
    let next_model = parse_model(next_source)?;
    let replacement: Field = next_model
        .entities
        .get(&resolved.entity_id)
        .and_then(|entity| entity.field(&resolved.field_id))
        .cloned()
        .ok_or_else(|| {
            Failure::Told(format!(
                "evolved field `{}` did not link",
                resolved.field_id
            ))
        })?;
    let patch = ModelPatch::ReplaceField {
        entity: resolved.entity_id.clone(),
        field: resolved.field_id.clone(),
        replacement: replacement.clone(),
        policy,
    };
    let patch_bytes = serde_json::to_vec(&json!({
        "patch": patch_json,
        "entity": resolved.entity_id,
        "field": resolved.field_id,
        "replacement": replacement,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation_with_reader_paths(
        PreparedMutation {
            name: format!("{}.{}", resolved.entity_label, display_name),
            invocation,
            model_path: resolved.model_path,
            current_source: resolved.current_source,
            current_model: resolved.current_model,
            next_source: std::mem::take(next_source),
            patch,
            patch_bytes,
            authored_migration: None,
        },
        reader_paths,
    )
}

fn reject_package(package: Option<&str>) -> Result<()> {
    if package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
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
        &resolved.current_source,
        &resolved.entity_java_name,
        &resolved.field_java_name,
        resolved.field_id.as_str(),
        next_name,
        next_column,
        &accepted_index_names(resolved),
    )
}

/// The SQL name each of this entity's indexes already has, keyed by the column
/// list its declaration spells.
///
/// Renaming a covered field moves the list, and the SQL name is derived from
/// it -- so without this the rename would quietly rename a database index too.
fn accepted_index_names(resolved: &ResolvedField) -> std::collections::BTreeMap<String, String> {
    let Some(entity) = resolved.current_model.entities.get(&resolved.entity_id) else {
        return std::collections::BTreeMap::new();
    };
    entity
        .indexes
        .values()
        .map(|index| {
            let columns = index
                .columns
                .iter()
                .map(|column| match column.direction {
                    jails_model::IndexDirection::Asc => {
                        entity_field_label(entity, &column.field).to_string()
                    }
                    jails_model::IndexDirection::Desc => {
                        format!("{} desc", entity_field_label(entity, &column.field))
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            (
                jdl_edit::normalize_columns(&columns),
                index.sql_name.clone(),
            )
        })
        .collect()
}

fn entity_field_label<'a>(
    entity: &'a jails_model::Entity,
    field: &jails_model::FieldId,
) -> &'a str {
    entity
        .fields
        .iter()
        .find(|candidate| &candidate.id == field)
        .map_or("", |candidate| candidate.label.as_str())
}

fn parse_model(source: &str) -> Result<AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
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
