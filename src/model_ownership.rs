//! `jails model status` -- which files jails owns, and whether each still
//! matches what it wrote.
//!
//! **The lock is the list.** Managed output lives beside the reader's own
//! sources under `src/`, and nothing about a path says whose it is; the
//! accepted projection in `.jails/compiler.lock.json` names every managed
//! file with the bytes it was accepted at. Listing the old generated root used to be
//! the way to see what jails owns, and this is its replacement: one line per
//! path, from the lock, with the artifact it was rendered from.
//!
//! **Drift is measured against the accepted image, never a fresh render**,
//! for `managed_drift`'s reason: a merge deliberately preserves reader edits,
//! so re-rendering and diffing would report every preserved edit as drift on
//! every run forever. `edited` here means the reader's delta is live and the
//! next generation merges over it; `missing` means the next generation
//! refuses, and `jails resource repair` writes it back.

use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use serde_json::json;

const SCHEMA: &str = "jails.model-status.v1";

/// One managed file against its accepted image.
#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    /// Byte for byte what the lock accepted.
    Managed,
    /// On disk with a reader delta the next generation merges over.
    Edited,
    /// Named by the lock and gone from the tree.
    Missing,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Edited => "edited",
            Self::Missing => "missing",
        }
    }
}

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let manifest = crate::model_command::resolve_manifest(None)?;
    let root = invocation.root()?;
    let (source, model) = crate::model_command::load_model(&root, &manifest, invocation.output)?;
    let snapshot = jails_project::capture::capture(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        None,
        &[],
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let rows = snapshot
        .accepted_projection
        .as_ref()
        .into_iter()
        .flat_map(|projection| projection.files.iter())
        .map(|(path, accepted)| {
            let state = match snapshot.files.get(path) {
                None => State::Missing,
                Some(live) if live.bytes == accepted.bytes => State::Managed,
                Some(_) => State::Edited,
            };
            (
                path.as_str().to_string(),
                accepted.provenance.artifact_id.clone(),
                state,
            )
        })
        .collect::<Vec<_>>();

    if invocation.output != Output::Human {
        return crate::model_command::print_json(&json!({
            "schema": SCHEMA,
            "accepted": snapshot.accepted_projection.is_some(),
            "files": rows
                .iter()
                .map(|(path, artifact, state)| json!({
                    "path": path,
                    "artifact": artifact,
                    "state": state.label(),
                }))
                .collect::<Vec<_>>(),
        }));
    }
    if snapshot.accepted_projection.is_none() {
        println!("no accepted projection yet: nothing is managed until the first plan is applied");
        return Ok(());
    }
    if rows.is_empty() {
        println!("the accepted projection names no file");
        return Ok(());
    }
    for (path, artifact, state) in &rows {
        println!("  {:<8} {path}  {artifact}", state.label());
    }
    let count = |wanted: State| rows.iter().filter(|(_, _, state)| *state == wanted).count();
    println!(
        "{} managed file{}, {} edited, {} missing",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" },
        count(State::Edited),
        count(State::Missing)
    );
    Ok(())
}
