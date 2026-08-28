//! Canonical `set` and `unset` frontends.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, SettingId, SettingTarget, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

const MODEL_PATH: &str = ".jails/model.toml";

pub(crate) fn owns() -> bool {
    crate::model_command::owns()
}

pub(crate) fn set(key: String, value: String, tests: bool, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let target = target(tests);
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
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
        });
    }

    let (id, label, mut next_source) = match existing {
        Some(setting) => (
            setting.id.clone(),
            setting.label.clone(),
            if jdl {
                crate::model_generate_jdl::remove_setting(
                    &current_source,
                    &setting.key,
                    setting.id.as_str(),
                )?
            } else {
                jails_model::remove_setting_declaration(&current_source, &setting.label)
                    .map_err(Failure::Told)?
            },
        ),
        None => {
            let identity = format!("{}:{key}", target.label());
            let label = format!("set_{}", &hex(&sha256(identity.as_bytes()))[..16]);
            let id = SettingId::parse(label.clone()).map_err(Failure::Told)?;
            (id, label, current_source.clone())
        }
    };
    append_setting(&mut next_source, &label, &id, &key, &value, target, jdl)?;
    let next_model = parse_model(&next_source, jdl)?;
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
    })
}

pub(crate) fn unset(key: String, tests: bool, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let target = target(tests);
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
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
    let next_source = if jdl {
        crate::model_generate_jdl::remove_setting(
            &current_source,
            &setting.key,
            setting.id.as_str(),
        )?
    } else {
        jails_model::remove_setting_declaration(&current_source, &setting.label)
            .map_err(Failure::Told)?
    };
    parse_model(&next_source, jdl)?;
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
    })
}

fn append_setting(
    source: &mut String,
    label: &str,
    id: &SettingId,
    key: &str,
    value: &str,
    target: SettingTarget,
    jdl: bool,
) -> Result<()> {
    if !source.ends_with('\n') {
        source.push('\n');
    }
    if jdl {
        source.push_str(&format!(
            "\nsetting {key} @id({}) @target({}) = {}\n",
            id.as_str(),
            target.label(),
            quote(value)?,
        ));
    } else {
        source.push_str(&format!(
            "\n[settings.{label}]\nid = {}\nkey = {}\nvalue = {}\ntarget = {}\n",
            quote(id.as_str())?,
            quote(key)?,
            quote(value)?,
            quote(target.label())?,
        ));
    }
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
    std::fs::read_to_string(path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{MODEL_PATH}`: {error}"
        ))
    })
}

fn model_path(jdl: bool) -> PathBuf {
    PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
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

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
