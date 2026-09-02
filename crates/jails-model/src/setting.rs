//! Linking rules for reader-document settings in the semantic model.

use crate::id::SettingId;
use crate::linker::Linker;
use crate::model::Setting;
use crate::source;
use std::collections::BTreeMap;

pub(crate) fn link(
    declarations: BTreeMap<String, source::Setting>,
    linker: &mut Linker,
) -> BTreeMap<SettingId, Setting> {
    let mut settings = BTreeMap::new();
    let mut keys = BTreeMap::<(crate::model::SettingTarget, String), String>::new();
    for (label, setting) in declarations {
        let path = format!("$.settings.{label}");
        linker.label(&label, &path);
        linker.register_id(&setting.id, &format!("{path}.id"));
        let id = linker.setting_id(&setting.id, &format!("{path}.id"));
        if !valid_key(&setting.key) {
            linker.problem(
                "model-setting-key",
                format!("{path}.key"),
                format!("`{}` is not a canonical properties key", setting.key),
                "use ASCII letters, digits, `.`, `_`, or `-` without whitespace or separators",
            );
        }
        if setting.value.chars().any(char::is_control) {
            linker.problem(
                "model-setting-value",
                format!("{path}.value"),
                "setting values must be single-line text without control characters",
                "remove newlines and control characters from the value",
            );
        }
        let identity = (setting.target, setting.key.clone());
        if let Some(first) = keys.insert(identity, path.clone()) {
            linker.problem(
                "model-setting-collision",
                path,
                format!(
                    "setting key `{}` for `{}` is already declared at {first}",
                    setting.key,
                    setting.target.label()
                ),
                "keep one declaration for each target and setting key",
            );
        }
        if let Some(id) = id {
            settings.insert(
                id.clone(),
                Setting {
                    id,
                    label,
                    key: setting.key,
                    value: setting.value,
                    target: setting.target,
                },
            );
        }
    }
    settings
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
