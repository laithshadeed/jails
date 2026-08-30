//! What the emitters need from the workspace, and the order they run in.
//!
//! Split out of `lib.rs` by secret the second time that file crossed the
//! largest-module ceiling. `Compiler::compile` decides *what* the desired
//! state is; this decides who renders it and what each renderer is told about
//! a workspace it may not look at itself.
//!
//! Keeping them apart is what makes the next observed fact cheap: it is a
//! field on [`Observed`] and a line in [`Observed::of`], rather than another
//! parameter threaded through four signatures.

use crate::{CompileError, emit_capability, emit_component, emit_http, emit_java, emit_operation};
use jails_contracts::{ProjectPath, RenderedTree, WorkspaceSnapshot};

/// The workspace facts emission needs and a pure compiler may not observe.
///
/// A value rather than three more parameters, for the reason `spring::Slice`
/// is one on the legacy side: every one of these is captured once and consumed
/// together, and threading them individually is how a signature reaches eight
/// arguments one honest addition at a time.
pub(crate) struct Observed<'a> {
    /// The Boot version the project declares, if it is a Spring project.
    pub spring_boot: Option<&'a str>,
    /// Where this project keeps its compose file.
    pub compose_path: &'a ProjectPath,
    /// Whether the project ships `mvnw`, so generated CI and container builds
    /// invoke the build the way the project actually offers it.
    pub maven_wrapper: bool,
}

pub(crate) fn emit(
    model: &jails_model::AppModel,
    output: &mut RenderedTree,
    observed: &Observed<'_>,
) -> Result<(), CompileError> {
    emit_capability::lower_and_emit(model, output, observed)?;
    emit_java::lower_and_emit(model, output, observed.spring_boot.is_some())?;
    emit_operation::lower_and_emit(model, output)?;
    emit_component::lower_and_emit(model, output)?;
    emit_http::lower_and_emit(model, output)
}

pub(crate) fn compose_path(snapshot: &WorkspaceSnapshot) -> Result<ProjectPath, CompileError> {
    if let Some(path) = snapshot
        .accepted_projection
        .as_ref()
        .and_then(|projection| {
            projection.reader_facets.values().find(|facet| {
                matches!(
                    facet.kind,
                    jails_contracts::ReaderFacetKind::ComposeService { .. }
                )
            })
        })
        .map(|facet| facet.path.clone())
    {
        return Ok(path);
    }
    for candidate in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
    ] {
        let path = ProjectPath::parse(candidate).map_err(CompileError::new)?;
        if snapshot.files.contains_key(&path) {
            return Ok(path);
        }
    }
    ProjectPath::parse("compose.yaml").map_err(CompileError::new)
}
