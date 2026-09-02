//! Canonical `set` and `unset` frontends.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, SettingId, SettingTarget, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

pub(crate) fn set(key: String, value: String, tests: bool, invocation: Invocation) -> Result<()> {
    let target = target(tests);
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = read_source(&model_path)?;
    let current_model = crate::model_generate_jdl::parse(&current_source)?;
    let existing = current_model
        .settings
        .values()
        .find(|setting| setting.target == target && setting.key == key)
        .cloned();
    if existing
        .as_ref()
        .is_some_and(|setting| setting.value == value)
    {
        return finish_generation(PreparedMutation {
            name: key,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
            authored_migration: None,
        });
    }

    let (id, mut next_source) = match existing {
        Some(setting) => (
            setting.id.clone(),
            crate::model_generate_jdl::remove_setting(&current_source, &setting.label)?,
        ),
        None => {
            let identity = format!("{}:{key}", target.label());
            let label = format!("set_{}", &hex(&sha256(identity.as_bytes()))[..16]);
            let id = SettingId::parse(label).map_err(Failure::Told)?;
            (id, current_source.clone())
        }
    };
    append_setting(&mut next_source, &id, &key, &value, target)?;
    let next_model = crate::model_generate_jdl::parse(&next_source)?;
    let setting = next_model
        .settings
        .get(&id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new setting `{id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "set-setting",
        "setting": setting,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: key,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::SetSetting(setting),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn unset(key: String, tests: bool, invocation: Invocation) -> Result<()> {
    let target = target(tests);
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = read_source(&model_path)?;
    let current_model = crate::model_generate_jdl::parse(&current_source)?;
    let setting = current_model
        .settings
        .values()
        .find(|setting| setting.target == target && setting.key == key)
        .cloned()
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical {} setting `{key}` is not declared.\n       fix: unset a key declared under `[settings]` for target `{}`",
                target.label(),
                target.label()
            ))
        })?;
    let next_source = crate::model_generate_jdl::remove_setting(&current_source, &setting.label)?;
    crate::model_generate_jdl::parse(&next_source)?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "remove-setting",
        "setting": setting.id,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: key,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::RemoveSetting(setting.id),
        patch_bytes,
        authored_migration: None,
    })
}

fn append_setting(
    source: &mut String,
    id: &SettingId,
    key: &str,
    value: &str,
    target: SettingTarget,
) -> Result<()> {
    let declaration = format!(
        "prop {key} = {} @id({}){}",
        quote(value)?,
        id.as_str(),
        if target == SettingTarget::Test {
            " @target(test)"
        } else {
            ""
        },
    );
    *source = jails_model::append_jdl_declaration(source, &declaration)
        .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    Ok(())
}

fn target(tests: bool) -> SettingTarget {
    if tests {
        SettingTarget::Test
    } else {
        SettingTarget::Main
    }
}

fn read_source(path: &Path) -> Result<String> {
    crate::model_command::read_source(path)
}

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
