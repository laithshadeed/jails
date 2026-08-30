//! Canonical resource projection renames preserve semantic identity and storage.

use crate::Invocation;
use crate::cli::{ExternalRenamePolicy, RenameStrategy};
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

const MODEL_PATH: &str = ".jails/model.toml";

pub(crate) fn owns() -> bool {
    crate::model_command::owns()
}

pub(crate) struct Request {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) strategy: RenameStrategy,
    pub(crate) table: Option<String>,
    pub(crate) api: ExternalRenamePolicy,
    pub(crate) route: Option<String>,
}

pub(crate) fn run(request: Request, invocation: Invocation) -> Result<()> {
    if request.strategy != RenameStrategy::PreserveTable {
        return Err(Failure::Told(
            "canonical resource rename currently implements only `--strategy preserve-table`.\n       fix: use preserve-table, or wait for the typed cutover/rolling migration backend"
                .to_string(),
        ));
    }
    if request.table.is_some() {
        return Err(Failure::Told(
            "`--table` would change storage during a preserve-table rename.\n       fix: remove `--table`; the entity's accepted SQL projection remains unchanged"
                .to_string(),
        ));
    }
    if request.api != ExternalRenamePolicy::Preserve || request.route.is_some() {
        return Err(Failure::Told(
            "canonical preserve-table rename keeps external names unchanged.\n       fix: remove `--api rename` and `--route`; API cutover needs its own compatibility policy"
                .to_string(),
        ));
    }

    let jdl = crate::model_command::owns_jdl();
    let model_path = PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        MODEL_PATH
    });
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source, jdl)?;
    let selector = request.from.rsplit('.').next().unwrap_or_default();
    if selector.is_empty() {
        return Err(Failure::Told(
            "canonical resource rename needs a non-empty entity selector.\n       fix: pass an entity label or Java type after `rename resource`"
                .to_string(),
        ));
    }
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == selector || entity.names.java_type == selector)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{}` does not exist.\n       fix: name an entity label or Java type declared under `[entities]`",
                request.from
            ))
        })?;
    if entity.names.java_type == request.to {
        return Err(Failure::Told(format!(
            "canonical entity `{}` already projects to `{}`.\n       fix: choose a different Java type name",
            entity.label, request.to
        )));
    }

    let entity_id = entity.id.clone();
    let entity_label = entity.label.clone();
    let sql_table = entity.names.sql_table.clone();
    let next_source = if jdl {
        crate::model_generate_jdl::rename_entity(
            &current_source,
            &entity.names.java_type,
            &request.to,
            &entity_label,
            entity.id.as_str(),
            entity
                .facets
                .contains(&jails_model::Facet::Record)
                .then_some(sql_table.as_str()),
        )?
    } else {
        jails_model::set_entity_java_name(&current_source, &entity_label, &request.to)
            .map_err(Failure::Told)?
    };
    let next_model = parse_model(&next_source, jdl)?;
    let next_label = next_model
        .entities
        .get(&entity_id)
        .map(|entity| entity.label.clone())
        .ok_or_else(|| {
            Failure::Told(format!(
                "lossless model edit removed entity `{entity_id}`.\n       fix: restore the entity declaration and retry"
            ))
        })?;
    let patch = ModelPatch::RenameEntityProjection {
        entity: entity_id.clone(),
        label: Some(next_label),
        java: Some(request.to.clone()),
        table: None,
    };
    let mut proof = current_model.clone();
    proof.apply(patch.clone()).map_err(Failure::Told)?;
    if next_model != proof {
        return Err(Failure::Told(
            "lossless model edit did not produce the intended semantic rename.\n       fix: restore a canonical entity table and retry"
                .to_string(),
        ));
    }
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "rename-entity-projection",
        "entity": entity_id,
        "java": request.to,
        "table": sql_table,
        "storage": "preserved",
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;

    finish_generation(PreparedMutation {
        name: entity_label,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
        authored_migration: None,
    })
}

fn parse_model(source: &str, jdl: bool) -> Result<jails_model::AppModel> {
    let parsed = if jdl {
        jails_model::parse_jdl(source)
    } else {
        jails_model::parse_toml(source)
    };
    parsed.map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}
