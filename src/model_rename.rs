//! Canonical resource projection renames preserve semantic identity and storage.

use crate::Invocation;
use crate::cli::{ExternalRenamePolicy, RenameStrategy};
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

const MODEL_PATH: &str = ".jails/model.toml";

pub(crate) struct Request {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) strategy: RenameStrategy,
    pub(crate) table: Option<String>,
    pub(crate) api: ExternalRenamePolicy,
    pub(crate) route: Option<String>,
}

pub(crate) fn run(request: Request, invocation: Invocation) -> Result<()> {
    // **Two strategies, and the difference is one migration.** Preserving the
    // table renames the Java projection and leaves storage exactly as
    // accepted; a single cutover renames the table too and says so in one
    // forward `alter table ... rename to ...`. Rolling and expand/contract are
    // campaigns -- several plans with an attestation between them -- and a
    // campaign is not something one command can honestly claim to have done.
    let cutover = match request.strategy {
        RenameStrategy::PreserveTable => false,
        RenameStrategy::SingleCutover => true,
        _ => {
            return Err(Failure::Told(
                "canonical resource rename implements `--strategy preserve-table` and `single-cutover`.\n       fix: a rolling or expand/contract rename is a campaign of plans rather than one; run the cutover when the readers are ready"
                    .to_string(),
            ));
        }
    };
    if request.table.is_some() && !cutover {
        return Err(Failure::Told(
            "`--table` would change storage during a preserve-table rename.\n       fix: remove `--table`, or use `--strategy single-cutover` to move the table explicitly"
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
    let current_source = crate::model_command::read_source(&model_path)?;
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
            // Pinned only when the table stays. A cutover lets the SQL name
            // follow the new label, which is what makes the migration below
            // the *whole* of the storage change rather than half of it.
            (!cutover && entity.facets.contains(&jails_model::Facet::Record))
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
    let next_table = next_model
        .entities
        .get(&entity_id)
        .map(|entity| entity.names.sql_table.clone())
        .unwrap_or_else(|| sql_table.clone());
    let patch = ModelPatch::RenameEntityProjection {
        entity: entity_id.clone(),
        label: Some(next_label),
        java: Some(request.to.clone()),
        table: cutover.then(|| next_table.clone()),
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
        "table": next_table,
        "storage": if cutover { "single-cutover" } else { "preserved" },
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
        // The cutover's `alter table ... rename to` is *derived*: the patch
        // states the policy and the compiler emits the statement beside every
        // other schema change, so it lands in the reviewed plan rather than
        // being smuggled in beside it.
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
