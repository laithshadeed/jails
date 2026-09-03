//! `jails model relocate` -- move a project generated before managed output
//! lived under `src/`.
//!
//! One plan, previewed and executed like every other: the captured bytes of
//! every managed file the lock names under the old generated root, hand
//! edits included, published at the reader path; the lock rewritten to name
//! the new paths; the marked source-root block taken out of the build file.
//! `jails_workspace::relocate` builds it and is the one place the old root
//! is spelled; this frontend captures, observes every destination so a
//! reader file already there is refused by name, and hands the bundle to the
//! one executor.

use crate::model_generate::report::{report_plan, write_bundle};
use crate::{Invocation, Output};
use jails_support::{Failure, Result};

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = invocation.root()?;
    let manifest = crate::model_command::resolve_manifest(None)?;
    let (source, model) = crate::model_command::load_model(&root, &manifest, invocation.output)?;
    let mut snapshot = jails_project::capture::capture(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        None,
        &[],
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    let targets =
        jails_workspace::relocation_targets(&snapshot).map_err(jails_project::diagnosed)?;
    if targets.is_empty() {
        if invocation.output == Output::Human {
            println!("nothing to relocate: every managed file already lives under `src/`");
        } else {
            crate::model_command::print_json(&serde_json::json!({
                "schema": "jails.model-relocate.v1",
                "moved": [],
            }))?;
        }
        return Ok(());
    }
    // Every destination is observed before the plan exists, so a reader file
    // already there is a captured before-image the plan refuses over, not a
    // stale precondition the executor trips on.
    jails_project::capture::observe_rendered_paths(
        &root,
        &mut snapshot,
        targets.iter().map(|(_, destination)| destination),
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    let bundle = jails_workspace::relocate(&snapshot, jails_compiler::COMPILER_VERSION).map_err(
        |error| {
            Failure::diagnosed(
                error.code,
                format!("could not plan the relocation: {error}"),
            )
        },
    )?;

    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(&root, &bundle).map_err(|error| {
        Failure::diagnosed(
            error.code,
            format!("could not relocate managed output: {error}"),
        )
    })?;
    if invocation.output == Output::Human {
        for (old, new) in &targets {
            println!("  move    {old} -> {new}");
        }
        println!(
            "relocated {} managed file{} under `src/`: {}",
            targets.len(),
            if targets.len() == 1 { "" } else { "s" },
            execution.plan_digest.as_str()
        );
        return Ok(());
    }
    crate::model_command::print_json(&serde_json::json!({
        "schema": "jails.model-relocate.v1",
        "plan_digest": execution.plan_digest,
        "moved": targets
            .iter()
            .map(|(old, new)| serde_json::json!({ "from": old, "to": new }))
            .collect::<Vec<_>>(),
    }))
}
