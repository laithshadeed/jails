//! Top-level JDL declarations that lower directly to semantic model nodes.

use super::{annotation, first_word, label, problem};
use crate::Diagnostics;

pub(super) struct DependencyDraft {
    pub(super) label: String,
    pub(super) id: String,
    pub(super) group: String,
    pub(super) artifact: String,
    pub(super) version: Option<String>,
    pub(super) scope: String,
}

pub(super) struct SettingDraft {
    pub(super) label: String,
    pub(super) id: String,
    pub(super) key: String,
    pub(super) value: String,
    pub(super) target: String,
}

pub(super) struct EjectionDraft {
    pub(super) label: String,
    pub(super) id: String,
    pub(super) target: String,
}

pub(super) fn dependency(line_number: usize, line: &str) -> Result<DependencyDraft, Diagnostics> {
    let rest = line
        .strip_prefix("dependency ")
        .expect("caller recognized dependency")
        .trim();
    let (header, version) = match rest.split_once('=') {
        Some((header, encoded)) => (
            header.trim(),
            Some(parse_string(
                line_number,
                encoded.trim(),
                "dependency version",
            )?),
        ),
        None => (rest, None),
    };
    let coordinate = first_word(header);
    let (group, artifact) = coordinate.split_once(':').ok_or_else(|| {
        problem(
            line_number,
            format!("`{coordinate}` is not a dependency coordinate"),
            "write `dependency group:artifact`, optionally followed by `= \"version\"`",
        )
    })?;
    if group.is_empty() || artifact.is_empty() {
        return Err(problem(
            line_number,
            "the dependency group or artifact is empty",
            "write `dependency group:artifact`",
        ));
    }
    let id = annotation(header, "id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("dep_{}_{}", label(group), label(artifact)));
    let scope = annotation(header, "scope").unwrap_or("compile");
    if !matches!(scope, "compile" | "runtime" | "test") {
        return Err(problem(
            line_number,
            format!("`{scope}` is not a dependency scope"),
            "use `@scope(compile)`, `@scope(runtime)`, or `@scope(test)`",
        ));
    }
    Ok(DependencyDraft {
        label: annotation(header, "as").unwrap_or(&id).to_string(),
        id,
        group: group.to_string(),
        artifact: artifact.to_string(),
        version,
        scope: scope.to_string(),
    })
}

pub(super) fn setting(line_number: usize, line: &str) -> Result<SettingDraft, Diagnostics> {
    let rest = line
        .strip_prefix("setting ")
        .expect("caller recognized setting")
        .trim();
    let (header, encoded) = rest.split_once('=').ok_or_else(|| {
        problem(
            line_number,
            "a setting has no value",
            "write `setting server.port @id(set_port) = \"8080\"`",
        )
    })?;
    let header = header.trim();
    let key = first_word(header);
    if key.is_empty() {
        return Err(problem(
            line_number,
            "the setting key is empty",
            "write `setting server.port = \"8080\"`",
        ));
    }
    let id = annotation(header, "id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("set_{}", label(key)));
    let target = annotation(header, "target").unwrap_or("main");
    if !matches!(target, "main" | "test") {
        return Err(problem(
            line_number,
            format!("`{target}` is not a setting target"),
            "use `@target(main)` or `@target(test)`",
        ));
    }
    Ok(SettingDraft {
        label: annotation(header, "as").unwrap_or(&id).to_string(),
        id,
        key: key.to_string(),
        value: parse_string(line_number, encoded.trim(), "setting value")?,
        target: target.to_string(),
    })
}

pub(super) fn ejection(line_number: usize, line: &str) -> Result<EjectionDraft, Diagnostics> {
    let rest = line
        .strip_prefix("eject ")
        .expect("caller recognized ejection")
        .trim();
    let target = first_word(rest);
    if target.is_empty() {
        return Err(problem(
            line_number,
            "the ejection target is empty",
            "write `eject art_cap_fake_ent_task_repository @id(eject_task_fake)`",
        ));
    }
    let id = annotation(rest, "id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("eject_{}", label(target)));
    Ok(EjectionDraft {
        label: annotation(rest, "as").unwrap_or(&id).to_string(),
        id,
        target: target.to_string(),
    })
}

fn parse_string(line: usize, encoded: &str, description: &str) -> Result<String, Diagnostics> {
    serde_json::from_str(encoded).map_err(|_| {
        problem(
            line,
            format!("the {description} is not a quoted string"),
            format!("write the {description} as a JSON-style string such as `\"value\"`"),
        )
    })
}
