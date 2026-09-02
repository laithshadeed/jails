//! Canonical `set` and `unset` frontends.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, SettingId, SettingTarget, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};

pub(crate) fn set(key: String, value: String, tests: bool, invocation: Invocation) -> Result<()> {
    let target = target(tests);
    let current = crate::model_command::Current::load(&invocation)?;
    let existing = current
        .model
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
            next_source: current.source.clone(),
            current,
            patch: ModelPatch::Batch(Vec::new()),
            authored_migration: None,
            reader_paths: Vec::new(),
        });
    }

    let (id, mut next_source) = match existing {
        Some(setting) => (
            setting.id.clone(),
            crate::model_generate_jdl::remove_setting(&current.source, &setting.label)?,
        ),
        None => {
            let identity = format!("{}:{key}", target.label());
            let label = format!("set_{}", &hex(&sha256(identity.as_bytes()))[..16]);
            let id = SettingId::parse(label).map_err(Failure::Told)?;
            (id, current.source.clone())
        }
    };
    append_setting(&mut next_source, &id, &key, &value, target)?;
    let next_model = crate::model_command::parse(&next_source)?;
    let setting = next_model
        .settings
        .get(&id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new setting `{id}` did not link")))?;
    finish_generation(PreparedMutation {
        name: key,
        invocation,
        current,
        next_source,
        patch: ModelPatch::SetSetting(setting),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn unset(key: String, tests: bool, invocation: Invocation) -> Result<()> {
    let target = target(tests);
    let current = crate::model_command::Current::load(&invocation)?;
    let setting = current.model
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
    let next_source = crate::model_generate_jdl::remove_setting(&current.source, &setting.label)?;
    crate::model_command::parse(&next_source)?;
    finish_generation(PreparedMutation {
        name: key,
        invocation,
        current,
        next_source,
        patch: ModelPatch::RemoveSetting(setting.id),
        authored_migration: None,
        reader_paths: Vec::new(),
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

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
