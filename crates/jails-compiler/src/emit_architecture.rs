//! The project's architecture fitness suite, as the compiler's output.
//!
//! **This is the one generated test that fails on code jails did not write**,
//! and that is deliberate: the layering it checks is the layering
//! `generate::layout` puts files into, so a hand-written class that reaches
//! across a boundary is exactly what it exists to catch. The cost is the
//! adoption story, which the freeze store answers -- `archunit.properties`
//! points at [`FREEZE_STORE`] and refuses to *create* it, so recording today's
//! violations stays a decision the reader makes.
//!
//! **Nothing it writes reads `.jails`.** The freeze store is a test resource
//! and the reviewed exceptions are `[[architecture.allow]]` tables in
//! `jails.toml`, so `rm -rf .jails` leaves a project whose tests still run and
//! still mean the same thing. A generated test that reads jails' own state
//! directory is a test whose verdict changes when a reader deletes a folder
//! they were told holds only inputs.
//!
//! It is emitted whenever the model serves a resource, never "once on the
//! first scaffold". The compiler is pure and idempotent, so "once" has
//! nowhere to live; deriving it from the model is also the stronger property,
//! because a project that loses its last scaffold loses a suite that would
//! have been checking nothing.

use crate::Diagnostic;
use jails_contracts::{
    BuildDependency, FileKind, FileMode, Provenance, RenderedFile, RenderedTree,
};
use jails_model::{AppModel, DependencyScope, Facet, Package};
use std::collections::BTreeSet;

const TEST_ROOT: &str = jails_contracts::SourceRoot::TestJava.path();
const TEST_RESOURCE_ROOT: &str = jails_contracts::SourceRoot::TestResources.path();
const ARTIFACT: &str = "art_project_architecture";

/// Where the frozen violations are recorded: a checked-in test resource, under
/// the source root that already holds every other thing the tests read.
///
/// The generated suite spells the same path (`FREEZE_STORE` in the template),
/// and `the_freeze_store_is_spelled_once` below fails when the two part.
/// `jails architecture baseline` is the third spelling and cannot see this
/// crate; it holds itself to the properties file it finds on disk, and refuses
/// by name when that file points somewhere else.
const FREEZE_STORE: &str = "src/test/resources/archunit/frozen";

/// **`allowStoreCreation=false` is the load-bearing line.** It keeps recording
/// a baseline a deliberate act rather than something a green build does
/// quietly, which is the whole reason the strict suite is safe to generate
/// into somebody else's repository.
///
/// A function rather than a constant so [`FREEZE_STORE`] is spelled once on
/// this side of the file as well as in the suite that reads it.
fn archunit_properties() -> String {
    format!(
        "freeze.store.default.path={FREEZE_STORE}\n\
         freeze.store.default.allowStoreCreation=false\n\
         freeze.store.default.allowStoreUpdate=false\n"
    )
}

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
) -> Result<(), Diagnostic> {
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
    // Through the one Java shell, so this file carries the provenance header
    // every other managed file carries. It was the one generated `.java` that
    // did not, which made "is this jails'?" a question with two answers.
    insert(
        output,
        format!(
            "{TEST_ROOT}/{}/ArchitectureTest.java",
            base.replace('.', "/")
        ),
        crate::emit_java::JavaUnit::from_source(&body)
            .render(&format!("{ARTIFACT}_architecture"))
            .into_bytes(),
        FileKind::JavaTest,
        "architecture",
    )?;
    // The header every managed file carries: the artifact it was rendered
    // from, so the answer to "is this jails'" is inside the file as well as
    // in the lock. Part of BASE, so an edit to it is an ordinary edit.
    insert(
        output,
        format!("{TEST_RESOURCE_ROOT}/archunit.properties"),
        format!(
            "# Generated by jails from {ARTIFACT}_architecture-properties. Clean hand edits survive regeneration.\n{}",
            archunit_properties()
        )
        .into_bytes(),
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
) -> Result<(), Diagnostic> {
    output
        .insert(
            crate::refuse::project_path(path)?,
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
        .map_err(crate::refuse::duplicate_emission)
}

#[cfg(test)]
mod tests {
    use super::{FREEZE_STORE, archunit_properties};

    fn template() -> &'static str {
        crate::template!("spring/architecture_test_java.java").built_in
    }

    /// The properties file is built from [`FREEZE_STORE`], so it cannot part
    /// from it; the suite spells the path in Java and can. (`jails
    /// architecture baseline` is the third spelling, and holds itself to the
    /// properties file it reads off disk.)
    #[test]
    fn the_freeze_store_is_spelled_once() {
        assert!(
            template().contains(&format!("FREEZE_STORE = \"{FREEZE_STORE}\"")),
            "the generated suite points somewhere else"
        );
    }

    /// Two readers of `jails.toml`: `jails_project::config` refuses an unknown
    /// key when the tool reads the file, and this template refuses one when
    /// the *project's* tests read it. A key one knows and the other does not
    /// is a policy that is accepted by jails and rejected by the build it
    /// generated, so the two lists are held together here.
    #[test]
    fn the_allowance_keys_match_the_tool_s_reader() {
        let rendered = jails_model::ARCHITECTURE_ALLOW_KEYS
            .iter()
            .map(|key| format!("\"{key}\""))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(
            template().contains(&format!("Set.of({rendered})")),
            "the generated suite's allowance keys are not {rendered}"
        );
    }

    /// Nothing the suite writes may name jails' own state directory: the
    /// project has to build and mean the same thing after `rm -rf .jails`.
    #[test]
    fn nothing_generated_here_reads_the_state_directory() {
        assert!(!archunit_properties().contains(".jails"));
        assert!(!template().contains(".jails"));
    }
}
