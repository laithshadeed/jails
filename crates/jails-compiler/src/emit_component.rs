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
//! **The Java bodies are the templates the legacy generator uses**, pulled in
//! with `include_str!` from `templates/spring/`. `CLAUDE.md` states the rule
//! for the project files both engines write, and it is the same rule here: two
//! copies of a template drift on exactly the details nobody re-reads, and
//! neither drift is visible where anyone looks. One file, two readers.

use crate::CompileError;
use jails_contracts::{
    BuildDependency, FileKind, FileMode, ProjectPath, PropertyEntry, Provenance, RenderedFile,
    RenderedTree,
};
use jails_model::{
    AppModel, Component, ComponentKind, DependencyScope, Package, SettingTarget, StableId,
};
use std::collections::BTreeSet;

mod auth;
mod client;
mod fetcher;
mod idempotency;
mod job;
mod socket;
mod webhook;

const MAIN_ROOT: &str = ".jails/generated/main/java";
const TEST_ROOT: &str = ".jails/generated/test/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    for component in model.components.values() {
        let files = match component.kind {
            ComponentKind::Client => client::files(model, component)?,
            ComponentKind::Fetcher => fetcher::files(model, component)?,
            ComponentKind::Job => job::files(model, component)?,
            ComponentKind::Auth => auth::files(model, component)?,
            ComponentKind::Idempotency => idempotency::files(model, component)?,
            ComponentKind::Socket => socket::files(model, component)?,
            ComponentKind::Webhook => webhook::files(model, component)?,
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
    if let Some(shared) = job::scheduling(model)? {
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
    idempotency::migrations(accepted, next)
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
        }));
    }
    dependencies
}

/// The `application.properties` entries this model's components need.
pub(crate) fn properties(model: &AppModel, target: SettingTarget) -> Vec<PropertyEntry> {
    if target != SettingTarget::Main {
        return Vec::new();
    }
    model
        .components
        .values()
        .filter(|component| component.kind == ComponentKind::Client)
        .flat_map(client::properties)
        .collect()
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
    body: String,
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
            bytes: format!(
                "// Generated by jails from {artifact}. Clean hand edits survive regeneration.\n{body}"
            )
            .into_bytes(),
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
