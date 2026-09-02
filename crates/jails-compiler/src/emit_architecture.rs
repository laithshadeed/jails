//! The project's architecture fitness suite, as the compiler's output.
//!
//! **This is the one generated test that fails on code jails did not write**,
//! and that is deliberate: the layering it checks is the layering
//! `generate::layout` puts files into, so a hand-written class that reaches
//! across a boundary is exactly what it exists to catch. The cost is the
//! adoption story, which the freeze store answers -- `archunit.properties`
//! points at `.jails/architecture-baseline` and refuses to *create* it, so
//! recording today's violations stays a decision the reader makes.
//!
//! It is emitted whenever the model serves a resource, never "once on the
//! first scaffold". The compiler is pure and idempotent, so "once" has
//! nowhere to live; deriving it from the model is also the stronger property,
//! because a project that loses its last scaffold loses a suite that would
//! have been checking nothing.

use crate::CompileError;
use jails_contracts::{
    BuildDependency, FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree,
};
use jails_model::{AppModel, DependencyScope, Facet, Package};
use std::collections::BTreeSet;

const TEST_ROOT: &str = ".jails/generated/test/java";
const TEST_RESOURCE_ROOT: &str = ".jails/generated/test/resources";
const ARTIFACT: &str = "art_project_architecture";

/// **`allowStoreCreation=false` is the load-bearing line.** It keeps recording
/// a baseline a deliberate act rather than something a green build does
/// quietly, which is the whole reason the strict suite is safe to generate
/// into somebody else's repository.
const ARCHUNIT_PROPERTIES: &str = "\
freeze.store.default.path=.jails/architecture-baseline
freeze.store.default.allowStoreCreation=false
freeze.store.default.allowStoreUpdate=false
";

/// **Pinned, and test-scoped.** ArchUnit is not managed by the Boot BOM, so a
/// versionless `<dependency>` outside `spring-boot-starter-parent` makes Maven
/// refuse to read the pom at all.
pub(crate) fn dependency() -> BuildDependency {
    BuildDependency {
        group: "com.tngtech.archunit".to_string(),
        artifact: "archunit-junit5".to_string(),
        version: Some("1.5.0".to_string()),
        scope: DependencyScope::Test,
        optional: false,
    }
}

/// Whether this model has a layered application to check.
///
/// `Facet::Http` rather than "any entity": the suite's rules are about the
/// controller/service/repository/adapter split, and a project of plain records
/// has no boundary for them to be about.
pub(crate) fn applies(model: &AppModel) -> bool {
    model
        .entities
        .values()
        .any(|entity| entity.active && entity.facets.contains(&Facet::Http))
}

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), CompileError> {
    let templates = &snapshot.template_overrides;
    if !applies(model) {
        return Ok(());
    }
    let base = model.project.package_for(Package::Base);
    let packages = [
        ("pkg", base.clone()),
        ("domain", model.project.package_for(Package::Domain)),
        // **`repository`, not `app`.** The rule text names the package the
        // ports are actually in, and the layout puts them under `repository`.
        // A suite naming a package the project does not have passes by
        // checking nothing, which is the failure mode `allowEmptyShould(true)`
        // hides.
        ("app", model.project.package_for(Package::Repository)),
        ("service", model.project.package_for(Package::Service)),
        ("web", model.project.package_for(Package::Web)),
        ("adapters", model.project.package_for(Package::Adapters)),
        ("messaging", model.project.package_for(Package::Messaging)),
        ("clients", model.project.package_for(Package::Clients)),
        ("jobs", model.project.package_for(Package::Jobs)),
    ];
    let mut body = crate::template!("spring/architecture_test_java.java")
        .resolve(templates)?
        .to_string();
    for (key, value) in &packages {
        body = body.replace(&format!("{{{{{key}}}}}"), value);
    }
    insert(
        output,
        format!(
            "{TEST_ROOT}/{}/ArchitectureTest.java",
            base.replace('.', "/")
        ),
        body.into_bytes(),
        FileKind::JavaTest,
        "architecture",
    )?;
    insert(
        output,
        format!("{TEST_RESOURCE_ROOT}/archunit.properties"),
        ARCHUNIT_PROPERTIES.as_bytes().to_vec(),
        FileKind::Resource,
        "architecture-properties",
    )
}

fn insert(
    output: &mut RenderedTree,
    path: String,
    bytes: Vec<u8>,
    kind: FileKind,
    suffix: &str,
) -> Result<(), CompileError> {
    output
        .insert(
            ProjectPath::parse(path).map_err(CompileError::new)?,
            RenderedFile {
                kind,
                mode: FileMode::Regular,
                bytes,
                provenance: Provenance {
                    artifact_id: format!("{ARTIFACT}_{suffix}"),
                    ejection_id: None,
                    ejectable: true,
                    semantic_ids: BTreeSet::new(),
                    compiler_pass: "architecture".to_string(),
                },
            },
        )
        .map_err(CompileError::new)
}
