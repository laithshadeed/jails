//! Canonical capability frontends and compiler-owned capability profiles.

use crate::Invocation;
use crate::add::Capability as CliCapability;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{CapabilityId, DependencyId, DependencyScope, ModelPatch, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The `app { storage ... }` value a legacy capability label means, for the
/// two kinds JDL v1 states as an axis rather than a capability.
///
/// **`sqlite` is deliberately not here.** v1's closed capability registry
/// carries `cap sqlite` as well as `storage sqlite`, and the linker
/// materializes the capability from the axis rather than the other way round —
/// so the capability is the primary spelling and routing `add sqlite` to the
/// axis would change what a working command writes. Only `db` and `h2` have no
/// capability spelling at all, which is why only they were broken.
fn storage_axis(label: &str) -> Option<&'static str> {
    match label {
        "db" => Some("postgres"),
        "h2" => Some("h2"),
        _ => None,
    }
}

pub(crate) fn add(
    capabilities: Vec<CliCapability>,
    name: Option<String>,
    package: Option<String>,
    invocation: Invocation,
) -> Result<()> {
    // **The CLI's sugar, resolved where the legacy path resolves it.** A
    // reader types `--name transaction`; the model holds `java_name` to a real
    // Java type name and is right to. Capitalising here is the same fold
    // `jails g record transaction` does, and without it the same command
    // produced a project on one engine and a diagnostic on the other.
    let name = name.map(|name| crate::model_generate_jdl::java_type_name(&name));
    validate_request(&capabilities, name.as_deref(), package.as_deref())?;
    let requested = capabilities
        .iter()
        .map(|capability| capability.label())
        .collect::<Vec<_>>()
        .join(", ");
    let jdl = invocation.owns_jdl();
    let model_path = model_path(jdl);
    // Resolved against the invocation's project, so `jails new --app` can
    // install a manifest's capabilities into the one it is creating.
    let current_source = crate::model_command::read_source_at(&invocation.root()?, &model_path)?;
    let v1 = jdl && crate::model_generate_jdl::is_v1_source(&current_source);
    if v1 && package.is_some() {
        return Err(Failure::Told(
            "JDL v1 derives capability packages from the closed projection registry.\n       fix: remove `--package`; eject the implementation boundary if it needs a reader-owned destination"
                .to_string(),
        ));
    }
    let current_model = parse_model(&current_source, jdl)?;
    let mut next_source = current_source.clone();
    let mut patches = Vec::new();
    let mut encoded = Vec::new();
    for capability in capabilities {
        let label = capability.label();
        let identity_label = if v1 {
            name.as_ref().map_or_else(
                || label.to_string(),
                |name| format!("{label}_{}", crate::model_resource::java_to_label(name)),
            )
        } else {
            label.to_string()
        };
        let id = CapabilityId::parse(format!("cap_{identity_label}")).map_err(Failure::Told)?;
        if let Some(existing) = current_model
            .capabilities
            .values()
            .find(|existing| existing.kind == label && existing.name == name)
        {
            let requested_package = package.as_ref().map(|package| {
                if package.is_empty() {
                    current_model.project.base_package.clone()
                } else {
                    format!("{}.{}", current_model.project.base_package, package)
                }
            });
            if existing.name != name || existing.java_package != requested_package {
                return Err(Failure::Told(format!(
                    "canonical capability `{label}` is already declared with a different name or package.\n       fix: rerun it with the recorded projection, or edit the capability declaration and run `jails sync`"
                )));
            }
            continue;
        }
        if jdl {
            if v1 {
                // **The storage kinds are an axis in v1, not a capability.**
                // Its closed registry has no `db`, `h2` or `sqlite`, because
                // `storage postgres` is what the reader declares and `cap db`
                // is what the linker materializes from it. Appending one wrote
                // a model that no longer parsed -- `jails add db` refused on
                // every v1 project, and failed closed, so it simply did not
                // work.
                if let Some(storage) = storage_axis(label) {
                    // `--name` never reaches here: `validate_request` already
                    // limits it to the four packs that have a projection to
                    // override, and neither of these is one.
                    next_source =
                        jails_model::set_jdl_app_property(&next_source, "storage", storage)
                            .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
                    // **The patch has to carry the axis too.** The source is
                    // what the model is re-read from next time; the patch is
                    // what *this* transition compiles. Without it `add db` on a
                    // project that already had entities lowered them against
                    // `dialect none` and refused -- so the capability worked
                    // only as the very first command in a project.
                    let dialect = jails_model::parse_jdl(&next_source)
                        .map_err(|diagnostics| {
                            Failure::Told(diagnostics.to_string().trim_end().to_string())
                        })?
                        .project
                        .dialect;
                    patches.push(ModelPatch::SetDialect(dialect.clone()));
                    encoded.push(json!({"kind": "set-dialect", "dialect": dialect}));
                } else {
                    let declaration = format!(
                        "cap {label}{} @id({})",
                        name.as_ref()
                            .map(|name| format!(" {name}"))
                            .unwrap_or_default(),
                        id.as_str(),
                    );
                    next_source = jails_model::append_jdl_declaration(&next_source, &declaration)
                        .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
                }
            } else {
                append_legacy_jdl(
                    &mut next_source,
                    &format!(
                        "capability {label} @id({}){}{}",
                        id.as_str(),
                        name.as_ref()
                            .map(|name| format!(" @name({name})"))
                            .unwrap_or_default(),
                        package
                            .as_ref()
                            .map(|package| format!(" @package({package})"))
                            .unwrap_or_default(),
                    ),
                );
            }
        } else {
            if !next_source.ends_with('\n') {
                next_source.push('\n');
            }
            next_source.push_str(&format!(
                "\n[capabilities.{label}]\nid = {}\nkind = {}\n",
                quote(&format!("cap_{label}"))?,
                quote(label)?,
            ));
            if let Some(name) = &name {
                next_source.push_str(&format!("name = {}\n", quote(name)?));
            }
            if let Some(package) = &package {
                next_source.push_str(&format!("package = {}\n", quote(package)?));
            }
        }
        let next_model = parse_model(&next_source, jdl)?;
        let declaration = next_model
            .capabilities
            .get(&id)
            .cloned()
            .ok_or_else(|| Failure::Told(format!("new capability `{id}` did not link")))?;
        encoded.push(json!({"kind": "add-capability", "capability": declaration}));
        patches.push(ModelPatch::AddCapability(declaration));
    }
    let patch_bytes = serde_json::to_vec(&json!({"kind": "batch", "patches": encoded}))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: format!("capability {requested}"),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::Batch(patches),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn remove(
    capabilities: Vec<CliCapability>,
    name: Option<String>,
    package: Option<String>,
    invocation: Invocation,
) -> Result<()> {
    validate_supported(&capabilities)?;
    let requested = capabilities
        .iter()
        .map(|capability| capability.label())
        .collect::<Vec<_>>()
        .join(", ");
    let jdl = crate::model_command::owns_jdl();
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
    let mut next_source = current_source.clone();
    let mut patches = Vec::new();
    let mut encoded = Vec::new();
    for capability in capabilities {
        let label = capability.label();
        let declaration = current_model
            .capabilities
            .values()
            .find(|candidate| candidate.kind == label)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical capability `{label}` is not declared.\n       fix: remove a capability declared under `[capabilities]`"
                ))
            })?;
        if let Some(name) = &name
            && declaration.name.as_ref() != Some(name)
        {
            return Err(Failure::Told(format!(
                "canonical capability `{label}` is recorded with name `{}` rather than `{name}`.\n       fix: omit `--name`; the model already identifies the generated boundary",
                declaration.name.as_deref().unwrap_or("<default>")
            )));
        }
        if let Some(package) = &package {
            let expected = if package.is_empty() {
                current_model.project.base_package.clone()
            } else {
                format!("{}.{}", current_model.project.base_package, package)
            };
            if declaration.java_package.as_deref() != Some(expected.as_str()) {
                return Err(Failure::Told(format!(
                    "canonical capability `{label}` is not recorded in package `{package}`.\n       fix: omit `--package`; the model already identifies the generated boundary"
                )));
            }
        }
        let v1 = jdl && crate::model_generate_jdl::is_v1_source(&next_source);
        next_source = if v1 && storage_axis(label).is_some() {
            // **`remove` is the exact inverse of `add`, including here.**
            // `add h2` on a v1 project sets `storage h2` rather than
            // appending `cap h2`, because storage is an axis and the closed
            // capability registry has no `h2` in it. Removal went looking for
            // the declaration `add` had deliberately not written, and refused
            // with a diagnostic about a `cap h2` that was never going to
            // exist -- so a project could enter a storage it could not leave.
            //
            // `none` rather than deleting the line: `storage` is a required
            // member of `app`, and an axis with no value is not a v1 model.
            jails_model::set_jdl_app_property(&next_source, "storage", "none")
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?
        } else if jdl {
            crate::model_generate_jdl::remove_capability(
                &next_source,
                &declaration.kind,
                declaration.id.as_str(),
                &declaration.label,
            )?
        } else {
            jails_model::remove_capability_declaration(&next_source, &declaration.label)
                .map_err(Failure::Told)?
        };
        encoded.push(json!({"kind": "remove-capability", "capability": declaration.id}));
        patches.push(ModelPatch::RemoveCapability(declaration.id.clone()));
    }
    parse_model(&next_source, jdl)?;
    let patch_bytes = serde_json::to_vec(&json!({"kind": "batch", "patches": encoded}))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: format!("capability {requested}"),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::Batch(patches),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn add_dependency(
    coordinate: jails_protocol::coordinate::MavenCoordinate,
    version: Option<String>,
    scope: DependencyScope,
    invocation: Invocation,
) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let coordinate = coordinate.to_string();
    let (group, artifact) = coordinate
        .split_once(':')
        .expect("validated Maven coordinates contain one separator");
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let v1 = jdl && crate::model_generate_jdl::is_v1_source(&current_source);
    let current_model = parse_model(&current_source, jdl)?;
    if let Some(existing) = current_model
        .dependencies
        .values()
        .find(|dependency| dependency.group == group && dependency.artifact == artifact)
    {
        if existing.version != version || existing.scope != scope {
            return Err(Failure::Told(format!(
                "canonical dependency `{coordinate}` is already declared with a different version or scope.\n       fix: remove the dependency, then add it again with the desired options"
            )));
        }
        return finish_generation(PreparedMutation {
            name: coordinate,
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

    let suffix = &hex(&sha256(coordinate.as_bytes()))[..16];
    let label = format!("dep_{suffix}");
    let id = DependencyId::parse(label.clone()).map_err(Failure::Told)?;
    let mut next_source = current_source.clone();
    if v1 {
        let declaration = format!(
            "dep {coordinate} @id({}){}{}",
            id.as_str(),
            version
                .as_ref()
                .map(|version| quote(version).map(|version| format!(" @version({version})")))
                .transpose()?
                .unwrap_or_default(),
            if scope != DependencyScope::Compile {
                format!(" @scope({})", scope_name(scope))
            } else {
                String::new()
            },
        );
        next_source = jails_model::append_jdl_declaration(&next_source, &declaration)
            .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    } else if jdl {
        append_legacy_jdl(
            &mut next_source,
            &format!(
                "dependency {coordinate} @id({}) @scope({}){}",
                id.as_str(),
                scope_name(scope),
                version
                    .as_ref()
                    .map(|version| quote(version).map(|version| format!(" = {version}")))
                    .transpose()?
                    .unwrap_or_default(),
            ),
        );
    } else {
        if !next_source.ends_with('\n') {
            next_source.push('\n');
        }
        next_source.push_str(&format!(
            "\n[dependencies.{label}]\nid = {}\ngroup = {}\nartifact = {}\n",
            quote(id.as_str())?,
            quote(group)?,
            quote(artifact)?,
        ));
        if let Some(version) = &version {
            next_source.push_str(&format!("version = {}\n", quote(version)?));
        }
        next_source.push_str(&format!("scope = {}\n", quote(scope_name(scope))?));
    }
    let next_model = parse_model(&next_source, jdl)?;
    let dependency = next_model
        .dependencies
        .get(&id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new dependency `{id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-dependency",
        "dependency": dependency,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: coordinate,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddDependency(dependency),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn remove_dependency(
    coordinate: jails_protocol::coordinate::MavenCoordinate,
    invocation: Invocation,
) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let coordinate = coordinate.to_string();
    let (group, artifact) = coordinate
        .split_once(':')
        .expect("validated Maven coordinates contain one separator");
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
    let dependency = current_model
        .dependencies
        .values()
        .find(|dependency| dependency.group == group && dependency.artifact == artifact)
        .cloned()
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical dependency `{coordinate}` is not declared.\n       fix: remove a coordinate declared under `[dependencies]`"
            ))
        })?;
    let next_source = if jdl {
        crate::model_generate_jdl::remove_dependency(
            &current_source,
            &coordinate,
            dependency.id.as_str(),
            &dependency.label,
        )?
    } else {
        jails_model::remove_dependency_declaration(&current_source, &dependency.label)
            .map_err(Failure::Told)?
    };
    parse_model(&next_source, jdl)?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "remove-dependency",
        "dependency": dependency.id,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: coordinate,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::RemoveDependency(dependency.id),
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn ensure_fast_test(invocation: Invocation) -> Result<()> {
    set_tool_capability("fast_test", "fast-test", true, invocation)
}

pub(crate) fn remove_fast_test(invocation: Invocation) -> Result<()> {
    set_tool_capability("fast_test", "fast-test", false, invocation)
}

fn set_tool_capability(
    label: &str,
    kind: &str,
    present: bool,
    invocation: Invocation,
) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let model_path = model_path(jdl);
    let current_source = read_source(&model_path)?;
    let v1 = jdl && crate::model_generate_jdl::is_v1_source(&current_source);
    let current_model = parse_model(&current_source, jdl)?;
    let existing = current_model
        .capabilities
        .values()
        .find(|capability| capability.kind == kind)
        .cloned();
    let (next_source, patch, encoded) = match (present, existing) {
        (true, Some(_)) | (false, None) => (
            current_source.clone(),
            ModelPatch::Batch(Vec::new()),
            json!({"kind": "batch", "patches": []}),
        ),
        (true, None) => {
            let id = CapabilityId::parse(format!("cap_{label}")).map_err(Failure::Told)?;
            let mut next = current_source.clone();
            if v1 {
                next = jails_model::append_jdl_declaration(
                    &next,
                    &format!("cap {kind} @id({})", id.as_str()),
                )
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
            } else if jdl {
                append_legacy_jdl(
                    &mut next,
                    &format!("capability {kind} @id({})", id.as_str()),
                );
            } else {
                if !next.ends_with('\n') {
                    next.push('\n');
                }
                next.push_str(&format!(
                    "\n[capabilities.{label}]\nid = {}\nkind = {}\n",
                    quote(id.as_str())?,
                    quote(kind)?,
                ));
            }
            let linked = parse_model(&next, jdl)?;
            let capability = linked
                .capabilities
                .get(&id)
                .cloned()
                .ok_or_else(|| Failure::Told(format!("new capability `{id}` did not link")))?;
            let encoded = json!({"kind": "add-capability", "capability": capability});
            (next, ModelPatch::AddCapability(capability), encoded)
        }
        (false, Some(capability)) => {
            let next = if jdl {
                crate::model_generate_jdl::remove_capability(
                    &current_source,
                    &capability.kind,
                    capability.id.as_str(),
                    &capability.label,
                )?
            } else {
                jails_model::remove_capability_declaration(&current_source, &capability.label)
                    .map_err(Failure::Told)?
            };
            parse_model(&next, jdl)?;
            let encoded = json!({"kind": "remove-capability", "capability": capability.id});
            (next, ModelPatch::RemoveCapability(capability.id), encoded)
        }
    };
    let patch_bytes = serde_json::to_vec(&encoded)
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: "fast-test".to_string(),
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

fn append_legacy_jdl(source: &mut String, declaration: &str) {
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push('\n');
    source.push_str(declaration);
    source.push('\n');
}

fn scope_name(scope: DependencyScope) -> &'static str {
    match scope {
        DependencyScope::Compile => "compile",
        DependencyScope::Runtime => "runtime",
        DependencyScope::Test => "test",
    }
}

fn validate_request(
    capabilities: &[CliCapability],
    name: Option<&str>,
    package: Option<&str>,
) -> Result<()> {
    validate_supported(capabilities)?;
    if (name.is_some() || package.is_some())
        && (capabilities.len() != 1
            || !matches!(
                capabilities.first(),
                Some(
                    CliCapability::Csv
                        | CliCapability::Json
                        | CliCapability::Http
                        | CliCapability::Sqlite
                )
            ))
    {
        return Err(Failure::Told(
            "canonical `--name` and `--package` currently belong to one `csv`, `json`, `http`, or `sqlite` capability pack.\n       fix: add one named capability at a time, or remove those projection overrides"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_supported(capabilities: &[CliCapability]) -> Result<()> {
    if capabilities.is_empty()
        || capabilities
            .iter()
            .any(|capability| !crate::canonical_support::capability(*capability).is_native())
    {
        let asked = capabilities
            .iter()
            .map(|capability| capability.label())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Failure::Told(format!(
            "canonical capability backend is not implemented for `{asked}`.\n       fix: use compiler-owned `jails add fake`, `jails add db`, `jails add api`, `jails add csv`, `jails add json`, `jails add http`, `jails add sqlite`, `jails add h2`, `jails add actuator`, `jails add cache`, `jails add coverage`, `jails add cors`, `jails add observability`, `jails add security`, `jails add sse`, `jails add redis`, `jails add kafka`, `jails add mail`, `jails add testkit`, `jails add toxiproxy`, `jails add loadtest`, `jails add ci`, `jails add docker`, `jails add k8s`, or `jails add format`; other capabilities refuse instead of entering the legacy planner"
        )));
    }
    Ok(())
}

fn read_source(path: &Path) -> Result<String> {
    crate::model_command::read_source(path)
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
