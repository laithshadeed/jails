//! Framework-shaped components, lowered to Java.
//!
//! **Why this is not `emit_unit.rs`.** A [`jails_model::SourceUnit`] is one
//! plain Java type: a package, a name, maybe variants and an endpoint.
//! `linker::component` projects the eight unit-shaped component kinds onto
//! that and returns `None` for the rest, and the rest are not shaped like it —
//! `client` is an interface *and* a registration bean *and* a test *and* a
//! build dependency *and* three properties, from one declaration. Projecting
//! it through `SourceUnit` would drop the last two before the emitter ever saw
//! them.
//!
//! **The Java bodies are the templates under `templates/spring/`**, pulled in
//! with `include_str!`: two copies of a template drift on exactly the details
//! nobody re-reads, and neither drift is visible where anyone looks.

use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_contracts::{
    BuildDependency, FileKind, FileMode, ProjectPath, PropertyEntry, Provenance, RenderedFile,
    RenderedTree,
};
use jails_model::{
    AppModel, Component, ComponentKind, DependencyScope, Package, SettingTarget, StableId,
};
use std::collections::BTreeSet;

mod auth;
mod cli;
mod client;
mod command;
mod durable_job;
mod fetcher;
mod handler;
pub(crate) mod http_sink;
mod http_workflow;
mod idempotency;
mod job;
mod presence;
mod socket;
mod webhook;

const MAIN_ROOT: &str = ".jails/generated/main/java";
const TEST_ROOT: &str = ".jails/generated/test/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<(), CompileError> {
    for component in model.components.values() {
        let files = match component.kind {
            ComponentKind::Client => client::files(model, component, templates)?,
            ComponentKind::Fetcher => fetcher::files(model, component, templates)?,
            ComponentKind::Job => job::files(model, component, templates)?,
            ComponentKind::Auth => auth::files(model, component, templates)?,
            ComponentKind::Idempotency => idempotency::files(model, component, templates)?,
            ComponentKind::Handler => handler::files(model, component, templates)?,
            ComponentKind::Presence => presence::files(model, component, templates)?,
            ComponentKind::Command => command::files(model, component, templates)?,
            ComponentKind::Cli => cli::files(model, component, templates)?,
            ComponentKind::Socket => socket::files(model, component, templates)?,
            ComponentKind::Webhook => webhook::files(model, component, templates)?,
            ComponentKind::HttpSink => http_sink::files(model, component, templates)?,
            ComponentKind::HttpWorkflow => http_workflow::files(model, component, templates)?,
            ComponentKind::DurableJob => durable_job::files(model, component, templates)?,
            _ => continue,
        };
        for file in files {
            output
                .insert(file.path, file.file)
                .map_err(CompileError::new)?;
        }
    }
    // Emitted after the loop and once: `SchedulingConfig` belongs to every job
    // in the model rather than to one, and a managed tree refuses two units
    // writing the same path.
    if let Some(shared) = job::scheduling(model, templates)? {
        output
            .insert(shared.path, shared.file)
            .map_err(CompileError::new)?;
    }
    for shared in handler::envelope(model, templates)? {
        output
            .insert(shared.path, shared.file)
            .map_err(CompileError::new)?;
    }
    Ok(())
}

/// The forward migrations this model's components need.
///
/// Takes the accepted model because a migration is an irreproducible
/// operation: what matters is which components are *new*, not which exist.
pub(crate) fn migrations(
    accepted: Option<&AppModel>,
    next: &AppModel,
) -> Vec<jails_contracts::RenderedMigration> {
    let mut migrations = idempotency::migrations(accepted, next);
    migrations.extend(presence::migrations(accepted, next));
    migrations.extend(http_workflow::migrations(accepted, next));
    migrations.extend(durable_job::migrations(accepted, next));
    migrations
}

/// The entry point a `cli` component may claim, if jails may claim one.
///
/// `model` is the *intended* model, not `snapshot.model.model`: the command
/// that declares a `cli` is the one whose pre-patch model has none, so reading
/// the snapshot's means `jails g cli Admin` never retargets `<mainClass>` and
/// some later, unrelated command does it instead. The snapshot is still what
/// says whether jails *may* claim the entry point -- that answer is about the
/// pom on disk.
pub(crate) fn entry_point(
    snapshot: &jails_contracts::WorkspaceSnapshot,
    model: &AppModel,
) -> Option<String> {
    cli::entry_point(snapshot, model)
}

/// Whether SQL is reachable from this project, however it got there.
///
/// **Not "did jails install the database".** A project can carry the JDBC
/// starter because the reader declared it -- a Spring application running on
/// an H2 file -- and a component that refuses there is refusing over a
/// database that is present. What the `db` capability additionally supplies
/// is a `TestcontainersConfig`, which is a separate question with a separate
/// answer in [`container_support`].
fn has_database(model: &AppModel) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
        || model
            .dependencies
            .values()
            .any(|dependency| JDBC_STARTERS.contains(&dependency.artifact.as_str()))
}

/// The artifacts that put a `DataSource` and `JdbcClient` on the classpath.
const JDBC_STARTERS: [&str; 2] = ["spring-boot-starter-jdbc", "spring-boot-starter-data-jdbc"];

/// What an integration test needs to reach a container config.
///
/// One decision: either the `@Import` or `@Disabled`, with whatever each
/// names. Emitting the annotation over a config the model never declared hands
/// the reader a `cannot find symbol` in a file they did not write, and
/// emitting nothing drops the coverage silently.
///
/// The imports are names for the unit's set rather than statements, so an
/// integration test that also imports something of its own gets one block.
fn container_support(model: &AppModel) -> ContainerSupport {
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return ContainerSupport {
            container: None,
            annotation: "",
            disabled: "@Disabled(\"todo: run jails add db to generate TestcontainersConfig, \
                       or point this at the database this project already has\")\n",
        };
    }
    ContainerSupport {
        container: Some(model.project.package_for(Package::Base)),
        annotation: "@Import(TestcontainersConfig.class)\n",
        disabled: "",
    }
}

struct ContainerSupport {
    /// The package `TestcontainersConfig` is in, when there is one.
    container: Option<String>,
    annotation: &'static str,
    disabled: &'static str,
}

impl ContainerSupport {
    /// Add what the annotation this support renders names.
    fn declare(&self, unit: &mut JavaUnit) {
        match &self.container {
            Some(base) => {
                unit.import("org.springframework.context.annotation.Import");
                unit.import_from(base, "TestcontainersConfig");
            }
            None => unit.import("org.junit.jupiter.api.Disabled"),
        }
    }
}

/// The build dependencies this model's components need.
///
/// Every one is versionless, which is correct under
/// `spring-boot-starter-parent` and required rather than merely tidy: a
/// `<version>` invented here would pin a starter against the reader's Boot.
pub(crate) fn dependencies(model: &AppModel) -> Vec<BuildDependency> {
    let mut dependencies = Vec::new();
    for (kind, required) in [
        (ComponentKind::Client, client::DEPENDENCIES),
        (ComponentKind::Fetcher, fetcher::DEPENDENCIES),
        (ComponentKind::Socket, socket::DEPENDENCIES),
        (ComponentKind::HttpSink, http_sink::DEPENDENCIES),
        (ComponentKind::HttpWorkflow, http_workflow::DEPENDENCIES),
    ] {
        if !model
            .components
            .values()
            .any(|component| component.kind == kind)
        {
            continue;
        }
        dependencies.extend(required.iter().map(|(group, artifact)| BuildDependency {
            group: (*group).to_string(),
            artifact: (*artifact).to_string(),
            version: None,
            scope: DependencyScope::Compile,
            optional: false,
        }));
    }
    dependencies
}

/// The `application.properties` entries this model's components need.
pub(crate) fn properties(
    model: &AppModel,
    target: SettingTarget,
) -> Result<Vec<PropertyEntry>, CompileError> {
    if target != SettingTarget::Main {
        return Ok(Vec::new());
    }
    let mut properties = Vec::new();
    for component in model.components.values() {
        match component.kind {
            ComponentKind::Client => properties.extend(client::properties(component)),
            ComponentKind::HttpSink => {
                properties.extend(http_sink::properties(model, component)?);
            }
            ComponentKind::Auth => properties.extend(auth::properties()),
            ComponentKind::Webhook => properties.extend(webhook::properties(component)),
            _ => {}
        }
    }
    Ok(properties)
}

/// One rendered file and where it goes.
struct Emitted {
    path: ProjectPath,
    file: RenderedFile,
}

/// A managed Java file for one component, identified by that component and a
/// suffix rather than by its path.
///
/// The artifact id is what the merge is keyed on, so it has to survive a
/// rename: `art_<component id>_<suffix>` moves with the declaration where a
/// path-derived id would look like a delete and an add.
fn java(
    component: &Component,
    suffix: &str,
    package: &str,
    type_name: &str,
    test: bool,
    ejectable: bool,
    unit: impl Into<JavaUnit>,
) -> Result<Emitted, CompileError> {
    let artifact = format!("art_{}_{}", component.id.as_str(), suffix);
    let root = if test { TEST_ROOT } else { MAIN_ROOT };
    let path = ProjectPath::parse(format!(
        "{root}/{}/{type_name}.java",
        package.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok(Emitted {
        path,
        file: RenderedFile {
            bytes: unit.into().render(&artifact).into_bytes(),
            kind: if test {
                FileKind::JavaTest
            } else {
                FileKind::JavaMain
            },
            mode: FileMode::Regular,
            provenance: Provenance {
                artifact_id: artifact,
                ejection_id: None,
                ejectable,
                semantic_ids: BTreeSet::from([component.id.as_str().to_string()]),
                compiler_pass: "components".to_string(),
            },
        },
    })
}

/// Where a component's Java goes.
fn package(model: &AppModel, package: Package) -> String {
    model.project.package_for(package)
}
