//! Explicit ownership transfer from canonical managed output to reader source.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{EjectionId, Evolution, StableId};
use jails_support::{Failure, Result};
use jails_support::{hex, sha256};

pub(crate) fn run(reference: String, invocation: Invocation) -> Result<()> {
    // Observed rather than assumed, and observed exactly the way `capture`
    // does it -- the emitters branch on the Boot version, so resolving the
    // ejection boundary against `spring_boot: None` finds none of a Spring
    // project's files. See `implementation_paths`.
    let root = crate::model_command::root()?;
    // Relative, because it becomes a `ProjectPath` in the plan; the read is
    // anchored to `root`. See `model_command::project_root`.
    let current = crate::model_command::Current::load(&invocation)?;
    // A readable boundary path (`Note.repo.fake`) resolves through the one
    // registry the linker reads, to the artifact id the compiler emits; an
    // artifact or node id is taken as written. The source keeps the path as
    // typed, and the linker resolves it again on every read.
    let semantic_id = match resolve(&current.model, &reference) {
        Ok(id) => id,
        Err(unresolved) => {
            return Err(Failure::Told(format!(
                "{}\n       fix: {}",
                unresolved.message, unresolved.fix
            )));
        }
    };
    if current
        .model
        .ejections
        .values()
        .any(|ejection| ejection.target == semantic_id)
    {
        return Err(Failure::Told(format!(
            "semantic target `{semantic_id}` is already reader-owned.\n       fix: edit its source under `src/main/java`; Jails will not reclaim it"
        )));
    }
    let build_system = jails_project::capture::observe_build_system(&root);
    let spring_boot = jails_project::capture::observe_spring_boot(&root, build_system);
    let reader_paths = jails_compiler::implementation_paths(
        &current.model,
        &semantic_id,
        spring_boot.as_deref(),
        root.join("mvnw").is_file(),
    )
    .map_err(|error| Failure::Told(format!("could not resolve ejection boundary: {error}")))?;
    if reader_paths.is_empty() {
        return Err(Failure::Told(format!(
            "artifact `{semantic_id}` emits no ejectable Java implementation.\n       fix: eject an `art_...` adapter implementation id; records and ports remain managed ABI"
        )));
    }

    let id = ejection_id(&semantic_id)?;
    let mut next_source = current.source.clone();
    if !next_source.ends_with('\n') {
        next_source.push('\n');
    }
    next_source.push_str(&format!("\neject {reference} @id({})\n", id.as_str()));
    finish_generation(PreparedMutation {
        name: reference,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths,
    })
}

/// The identity of the ejection naming one semantic target.
///
/// Derived from the target, so `jails model eject` and `jails adopt resource`
/// give one boundary one id whichever wrote the line.
pub(crate) fn ejection_id(semantic_id: &str) -> Result<EjectionId> {
    let label = format!("eject_{}", &hex(&sha256(semantic_id.as_bytes()))[..16]);
    EjectionId::parse(label)
        .map_err(|error| Failure::Told(format!("could not assign ejection identity: {error}")))
}

/// The artifact id `reference` names: itself when it is already an artifact
/// or node id, otherwise the boundary the registry resolves it to.
fn resolve(
    model: &jails_model::AppModel,
    reference: &str,
) -> std::result::Result<String, jails_model::boundary::Unresolved> {
    if reference.starts_with("art_")
        || model.capabilities.keys().any(|id| id.as_str() == reference)
        || model.components.keys().any(|id| id.as_str() == reference)
        || model.entities.keys().any(|id| id.as_str() == reference)
        || model.operations.keys().any(|id| id.as_str() == reference)
        || model.units.keys().any(|id| id.as_str() == reference)
    {
        return Ok(reference.to_string());
    }
    jails_model::boundary::resolve_in(model, reference)
}
