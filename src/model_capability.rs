//! Canonical capability frontends and compiler-owned capability profiles.

use crate::CapabilityKind;
use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{DependencyScope, Evolution};
use jails_support::{Failure, Result};

/// The `app { storage ... }` value a capability label means, for the two
/// kinds JDL v1 states as an axis rather than a capability.
///
/// **`sqlite` is deliberately not here.** v1's closed capability registry
/// carries `cap sqlite` as well as `storage sqlite`, and the linker
/// materializes the capability from the axis rather than the other way round —
/// so the capability is the primary spelling and routing `add sqlite` to the
/// axis would change what a working command writes. Only `db` and `h2` have no
/// capability spelling at all.
fn storage_axis(label: &str) -> Option<&'static str> {
    match label {
        "db" => Some("postgres"),
        "h2" => Some("h2"),
        _ => None,
    }
}

pub(crate) fn add(
    capabilities: Vec<CapabilityKind>,
    name: Option<String>,
    package: Option<String>,
    invocation: Invocation,
) -> Result<()> {
    // **The CLI's sugar, resolved at the CLI.** A reader types `--name
    // transaction`; the model holds `java_name` to a real Java type name and
    // is right to. Capitalising here is the same fold `jails g record
    // transaction` does.
    let name = name.map(|name| crate::model_generate_jdl::java_type_name(&name));
    validate_request(&capabilities, name.as_deref(), package.as_deref())?;
    let requested = capabilities
        .iter()
        .map(|capability| capability.label())
        .collect::<Vec<_>>()
        .join(", ");
    // Resolved against the invocation's project, so `jails new --app` can
    // install a manifest's capabilities into the one it is creating.
    let current = crate::model_command::Current::load(&invocation)?;
    if package.is_some() {
        return Err(Failure::Told(
            "JDL v1 derives capability packages from a closed list.\n       fix: remove `--package`; eject the implementation boundary if it needs a reader-owned destination"
                .to_string(),
        ));
    }
    let mut next_source = current.source.clone();
    for capability in capabilities {
        let label = capability.label();
        if let Some(existing) = current
            .model
            .capabilities
            .values()
            .find(|existing| existing.kind == label && existing.name == name)
        {
            let requested_package = package.as_ref().map(|package| {
                if package.is_empty() {
                    current.model.project.base_package.clone()
                } else {
                    format!("{}.{}", current.model.project.base_package, package)
                }
            });
            if existing.name != name || existing.java_package != requested_package {
                return Err(Failure::Told(format!(
                    "capability `{label}` is already declared with a different name or package.\n       fix: rerun it with the recorded name and package, or edit the capability declaration and run `jails sync`"
                )));
            }
            continue;
        }
        // **The storage kinds are an axis in v1, not a capability.** Its closed
        // registry has no `db`, `h2` or `sqlite`, because `storage postgres` is
        // what the reader declares and `cap db` is what the linker materializes
        // from it. Appending one writes a model that does not parse.
        if let Some(storage) = storage_axis(label) {
            // `--name` never reaches here: `validate_request` already limits it
            // to the four packs that have a projection to override, and neither
            // of these is one.
            next_source = jails_model::set_jdl_app_property(&next_source, "storage", storage)
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
        } else {
            // The derivation reads the same `<kind>[_<name>]` label back off
            // the line, so the attribute would be a pin displacing nothing.
            let declaration = format!(
                "cap {label}{}",
                name.as_ref()
                    .map(|name| format!(" {name}"))
                    .unwrap_or_default(),
            );
            next_source = jails_model::append_jdl_declaration(&next_source, &declaration)
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
        }
    }
    finish_generation(PreparedMutation {
        name: format!("capability {requested}"),
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn remove(
    capabilities: Vec<CapabilityKind>,
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
    let current = crate::model_command::Current::load(&invocation)?;
    let mut next_source = current.source.clone();
    for capability in capabilities {
        let label = capability.label();
        let declaration = current.model
            .capabilities
            .values()
            .find(|candidate| candidate.kind == label)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "capability `{label}` is not declared.\n       fix: name a capability `.jails/model.jdl` declares"
                ))
            })?;
        if let Some(name) = &name
            && declaration.name.as_ref() != Some(name)
        {
            return Err(Failure::Told(format!(
                "capability `{label}` is recorded with name `{}` rather than `{name}`.\n       fix: omit `--name`; the model already identifies the generated boundary",
                declaration.name.as_deref().unwrap_or("<default>")
            )));
        }
        if let Some(package) = &package {
            let expected = if package.is_empty() {
                current.model.project.base_package.clone()
            } else {
                format!("{}.{}", current.model.project.base_package, package)
            };
            if declaration.java_package.as_deref() != Some(expected.as_str()) {
                return Err(Failure::Told(format!(
                    "capability `{label}` is not recorded in package `{package}`.\n       fix: omit `--package`; the model already identifies the generated boundary"
                )));
            }
        }
        next_source = if storage_axis(label).is_some() {
            // **`remove` is the exact inverse of `add`, including here.**
            // `add h2` on a v1 project sets `storage h2` rather than
            // appending `cap h2`, because storage is an axis and the closed
            // capability registry has no `h2` in it, so removal must not go
            // looking for a `cap h2` declaration `add` never wrote -- a
            // project must be able to leave any storage it can enter.
            //
            // `none` rather than deleting the line: `storage` is a required
            // member of `app`, and an axis with no value is not a v1 model.
            jails_model::set_jdl_app_property(&next_source, "storage", "none")
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?
        } else {
            crate::model_generate_jdl::remove_capability(&next_source, &declaration.label)?
        };
    }
    finish_generation(PreparedMutation {
        name: format!("capability {requested}"),
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn add_dependency(
    coordinate: jails_spec::spec::coordinate::MavenCoordinate,
    version: Option<String>,
    scope: DependencyScope,
    invocation: Invocation,
) -> Result<()> {
    let coordinate = coordinate.to_string();
    let (group, artifact) = coordinate
        .split_once(':')
        .expect("validated Maven coordinates contain one separator");
    let current = crate::model_command::Current::load(&invocation)?;
    if let Some(existing) = current
        .model
        .dependencies
        .values()
        .find(|dependency| dependency.group == group && dependency.artifact == artifact)
    {
        if existing.version != version || existing.scope != scope {
            return Err(Failure::Told(format!(
                "dependency `{coordinate}` is already declared with a different version or scope.\n       fix: remove the dependency, then add it again with the desired options"
            )));
        }
        return finish_generation(PreparedMutation {
            name: coordinate,
            invocation,
            next_source: current.source.clone(),
            current,
            evolution: Evolution::none(),
            authored_migration: None,
            reader_paths: Vec::new(),
        });
    }

    // **A coordinate is its own identity**, so the id is the coordinate's
    // label rather than a hash of it: the parser derives the same thing from
    // the line, which is what lets the declaration go in without an `@id`.
    let mut next_source = current.source.clone();
    {
        let declaration = format!(
            "dep {coordinate}{}{}",
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
    }
    finish_generation(PreparedMutation {
        name: coordinate,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn remove_dependency(
    coordinate: jails_spec::spec::coordinate::MavenCoordinate,
    invocation: Invocation,
) -> Result<()> {
    let coordinate = coordinate.to_string();
    let (group, artifact) = coordinate
        .split_once(':')
        .expect("validated Maven coordinates contain one separator");
    let current = crate::model_command::Current::load(&invocation)?;
    let dependency = current.model
        .dependencies
        .values()
        .find(|dependency| dependency.group == group && dependency.artifact == artifact)
        .cloned()
        .ok_or_else(|| {
            Failure::Told(format!(
                "dependency `{coordinate}` is not declared.\n       fix: name a coordinate `.jails/model.jdl` declares"
            ))
        })?;
    let next_source =
        crate::model_generate_jdl::remove_dependency(&current.source, &dependency.label)?;
    finish_generation(PreparedMutation {
        name: coordinate,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn ensure_fast_test(invocation: Invocation) -> Result<()> {
    set_tool_capability("fast-test", true, invocation)
}

pub(crate) fn remove_fast_test(invocation: Invocation) -> Result<()> {
    set_tool_capability("fast-test", false, invocation)
}

fn set_tool_capability(kind: &str, present: bool, invocation: Invocation) -> Result<()> {
    let current = crate::model_command::Current::load(&invocation)?;
    let existing = current
        .model
        .capabilities
        .values()
        .find(|capability| capability.kind == kind)
        .cloned();
    let next_source = match (present, existing) {
        (true, Some(_)) | (false, None) => current.source.clone(),
        (true, None) => {
            jails_model::append_jdl_declaration(&current.source, &format!("cap {kind}"))
                .map_err(crate::model_generate_jdl::jdl_edit_failure)?
        }
        (false, Some(capability)) => {
            crate::model_generate_jdl::remove_capability(&current.source, &capability.label)?
        }
    };
    finish_generation(PreparedMutation {
        name: "fast-test".to_string(),
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

fn scope_name(scope: DependencyScope) -> &'static str {
    match scope {
        DependencyScope::Compile => "compile",
        DependencyScope::Runtime => "runtime",
        DependencyScope::Test => "test",
    }
}

fn validate_request(
    capabilities: &[CapabilityKind],
    name: Option<&str>,
    package: Option<&str>,
) -> Result<()> {
    validate_supported(capabilities)?;
    if (name.is_some() || package.is_some())
        && (capabilities.len() != 1
            || !matches!(
                capabilities.first(),
                Some(
                    CapabilityKind::Csv
                        | CapabilityKind::Json
                        | CapabilityKind::Http
                        | CapabilityKind::Sqlite
                )
            ))
    {
        return Err(Failure::Told(
            "`--name` and `--package` currently belong to one `csv`, `json`, `http`, or `sqlite` capability pack.\n       fix: add one named capability at a time, or remove those name and package overrides"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_supported(capabilities: &[CapabilityKind]) -> Result<()> {
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
            "capability backend is not implemented for `{asked}`.\n       fix: use compiler-owned `jails add fake`, `jails add db`, `jails add api`, `jails add csv`, `jails add json`, `jails add http`, `jails add sqlite`, `jails add h2`, `jails add actuator`, `jails add cache`, `jails add coverage`, `jails add cors`, `jails add observability`, `jails add security`, `jails add sse`, `jails add redis`, `jails add kafka`, `jails add mail`, `jails add testkit`, `jails add toxiproxy`, `jails add loadtest`, `jails add ci`, `jails add docker`, `jails add k8s`, or `jails add format`; other capabilities refuse instead of entering the legacy planner"
        )));
    }
    Ok(())
}

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
