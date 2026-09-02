//! Linking rules for build coordinates declared in the semantic model.

use crate::id::DependencyId;
use crate::linker::Linker;
use crate::model::Dependency;
use crate::source;
use std::collections::BTreeMap;

pub(crate) fn link(
    declarations: BTreeMap<String, source::Dependency>,
    linker: &mut Linker,
) -> BTreeMap<DependencyId, Dependency> {
    let mut dependencies = BTreeMap::new();
    let mut coordinates = BTreeMap::<String, String>::new();
    for (label, dependency) in declarations {
        let path = format!("$.dependencies.{label}");
        linker.label(&label, &path);
        linker.register_id(&dependency.id, &format!("{path}.id"));
        let id = linker.dependency_id(&dependency.id, &format!("{path}.id"));
        for (part, value) in [
            ("group", dependency.group.as_str()),
            ("artifact", dependency.artifact.as_str()),
        ] {
            if !valid_coordinate_part(value) {
                linker.problem(
                    "model-dependency-coordinate",
                    format!("{path}.{part}"),
                    format!("`{value}` is not a valid Maven coordinate {part}"),
                    "use only ASCII letters, digits, `.`, `_`, and `-`",
                );
            }
        }
        if dependency.version.as_deref().is_some_and(|version| {
            version.trim().is_empty() || version.chars().any(char::is_control)
        }) {
            linker.problem(
                "model-dependency-version",
                format!("{path}.version"),
                "dependency version must be a non-empty single-line value",
                "remove `version` to use dependency management, or provide a pinned version",
            );
        }
        let coordinate = format!("{}:{}", dependency.group, dependency.artifact);
        if let Some(first) = coordinates.insert(coordinate.clone(), path.clone()) {
            linker.problem(
                "model-dependency-collision",
                path,
                format!("dependency coordinate `{coordinate}` is already declared at {first}"),
                "keep one declaration for each dependency coordinate",
            );
        }
        if let Some(id) = id {
            dependencies.insert(
                id.clone(),
                Dependency {
                    id,
                    label,
                    group: dependency.group,
                    artifact: dependency.artifact,
                    version: dependency.version,
                    scope: dependency.scope,
                },
            );
        }
    }
    dependencies
}

fn valid_coordinate_part(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
