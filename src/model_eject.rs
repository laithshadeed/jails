//! Explicit ownership transfer from canonical managed output to reader source.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation_with_reader_paths};
use jails_model::{EjectionId, ModelPatch, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(crate) fn run(semantic_id: String, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let model_path = PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
    });
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source, jdl)?;
    if current_model
        .ejections
        .values()
        .any(|ejection| ejection.target == semantic_id)
    {
        return Err(Failure::Told(format!(
            "semantic target `{semantic_id}` is already reader-owned.\n       fix: edit its source under `src/main/java`; Jails will not reclaim it"
        )));
    }
    let reader_paths = jails_compiler::implementation_paths(&current_model, &semantic_id)
        .map_err(|error| Failure::Told(format!("could not resolve ejection boundary: {error}")))?;
    if reader_paths.is_empty() {
        return Err(Failure::Told(format!(
            "artifact `{semantic_id}` emits no ejectable Java implementation.\n       fix: eject an `art_...` adapter implementation id; records and ports remain managed ABI"
        )));
    }

    let label = format!("eject_{}", &hex(&sha256(semantic_id.as_bytes()))[..16]);
    let id = EjectionId::parse(label.clone())
        .map_err(|error| Failure::Told(format!("could not assign ejection identity: {error}")))?;
    let mut next_source = current_source.clone();
    if !next_source.ends_with('\n') {
        next_source.push('\n');
    }
    if jdl {
        next_source.push_str(&format!("\neject {semantic_id} @id({})\n", id.as_str()));
    } else {
        next_source.push_str(&format!(
            "\n[ejections.{label}]\nid = {}\ntarget = {}\n",
            quote(id.as_str())?,
            quote(&semantic_id)?,
        ));
    }
    let next_model = parse_model(&next_source, jdl)?;
    let ejection = next_model
        .ejections
        .get(&id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new ejection `{id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-ejection",
        "ejection": ejection,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation_with_reader_paths(
        PreparedMutation {
            name: semantic_id,
            invocation,
            model_path,
            current_source,
            current_model,
            next_source,
            patch: ModelPatch::AddEjection(ejection),
            patch_bytes,
            authored_migration: None,
        },
        &reader_paths,
    )
}

fn parse_model(source: &str, jdl: bool) -> Result<jails_model::AppModel> {
    let parsed = if jdl {
        jails_model::parse_jdl(source)
    } else {
        jails_model::parse_toml(source)
    };
    parsed.map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
