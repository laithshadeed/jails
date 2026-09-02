//! Explicit ownership transfer from canonical managed output to reader source.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{EjectionId, ModelPatch, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};

pub(crate) fn run(semantic_id: String, invocation: Invocation) -> Result<()> {
    // Observed rather than assumed, and observed exactly the way `capture`
    // does it -- the emitters branch on the Boot version, so resolving the
    // ejection boundary against `spring_boot: None` finds none of a Spring
    // project's files. See `implementation_paths`.
    let root = crate::model_command::root()?;
    // Relative, because it becomes a `ProjectPath` in the plan; the read is
    // anchored to `root`. See `model_command::project_root`.
    let current = crate::model_command::Current::load(&invocation)?;
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
    let build_system = jails_workspace::observe_build_system(&root);
    let spring_boot = jails_workspace::observe_spring_boot(&root, build_system);
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

    let label = format!("eject_{}", &hex(&sha256(semantic_id.as_bytes()))[..16]);
    let id = EjectionId::parse(label.clone())
        .map_err(|error| Failure::Told(format!("could not assign ejection identity: {error}")))?;
    let mut next_source = current.source.clone();
    if !next_source.ends_with('\n') {
        next_source.push('\n');
    }
    next_source.push_str(&format!("\neject {semantic_id} @id({})\n", id.as_str()));
    let next_model = crate::model_command::parse(&next_source)?;
    let ejection = next_model
        .ejections
        .get(&id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new ejection `{id}` did not link")))?;
    finish_generation(PreparedMutation {
        name: semantic_id,
        invocation,
        current,
        next_source,
        patch: ModelPatch::AddEjection(ejection),
        authored_migration: None,
        reader_paths,
    })
}
