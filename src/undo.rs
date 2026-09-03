//! `jails undo`: the last applied plan, run backwards.
//!
//! **Nothing is recomputed.** Every operation of an applied plan carries the
//! image it found as well as the image it wrote, because that is what makes a
//! plan reviewable, so the inverse is the same bundle read backwards --
//! `jails_workspace::invert` -- handed to the one executor. There is no
//! reverse renderer, no second file table and no model to re-link, which is
//! the same reason `destroy` is subtraction rather than a catalogue.
//!
//! **The executor is what makes it safe.** The inverse plan's preconditions
//! are the applied plan's after-images, so a managed file edited since, a
//! second command in between, or a `git checkout` all make it stale and it
//! refuses with nothing written.
//!
//! One command deep, deliberately. The bundle is kept at
//! `.jails/run/last-plan.json`, which the state `.gitignore` keeps out of
//! every commit: undoing the command you just ran is a convenience for the
//! person who ran it, and a stack of them would be a history the project does
//! not carry and `git` already does.

use crate::Invocation;
use jails_contracts::PlanBundle;
use jails_support::{Failure, Result};
use std::path::PathBuf;

const LAST_PLAN: &str = ".jails/run/last-plan.json";

/// Where the last applied plan is kept, for the one project this invocation
/// is about.
///
/// **A value rather than a root passed around**, because two of the three
/// things done with it -- keeping one, forgetting one -- are best-effort and
/// a `Result<PathBuf>` at each call site would put error handling where there
/// is no error to report.
pub(crate) struct Kept {
    path: PathBuf,
}

impl Kept {
    pub(crate) fn of(invocation: &Invocation) -> Option<Self> {
        invocation.root().ok().map(|root| Self {
            path: root.join(LAST_PLAN),
        })
    }

    /// Keep this bundle as the one `undo` would reverse.
    ///
    /// Best-effort by design: a project whose `.jails/run` cannot be written
    /// has applied its plan, and should not be told the command failed
    /// because a convenience could not be saved.
    pub(crate) fn remember(&self, bundle: &PlanBundle) {
        let Ok(encoded) = serde_json::to_vec(bundle) else {
            return;
        };
        let _ = jails_support::apply::put_in_scratch(&self.path, encoded);
    }

    /// Forget it, because what it describes is no longer what happened.
    pub(crate) fn forget(&self) {
        let _ = jails_support::apply::remove_from_scratch(&self.path);
    }

    fn read(&self) -> Result<PlanBundle> {
        let bytes = std::fs::read(&self.path).map_err(|_| {
            Failure::Told(format!(
                "there is no command to undo: jails keeps the last applied plan at `{LAST_PLAN}` and this project has none\n       fix: run a command that writes something first; `undo` reverses one command, and `git` is what reverses more"
            ))
        })?;
        serde_json::from_slice(&bytes).map_err(|error| {
            Failure::Told(format!(
                "the kept plan at `{LAST_PLAN}` could not be read: {error}\n       fix: delete it; a plan jails cannot read is one it must not act on"
            ))
        })
    }
}

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = invocation.root()?;
    let kept = Kept::of(&invocation).ok_or_else(|| {
        Failure::Told(
            "this directory is not a project, so there is nothing to undo.\n       fix: run `jails undo` inside a project"
                .to_string(),
        )
    })?;
    let bundle = kept.read()?;
    let inverse = jails_workspace::invert(&bundle).map_err(|error| {
        Failure::diagnosed(error.code, format!("could not invert the plan: {error}"))
    })?;
    if invocation.pretend {
        // The same preview every mutation prints, over the inverse: the
        // reader is reviewing a plan, and it is a plan.
        for line in crate::plan_delta::preview_lines(&inverse) {
            println!("{line}");
        }
        return Ok(());
    }
    let execution = jails_workspace::execute(&root, &inverse).map_err(|error| {
        // **The generic repair is the wrong one here.** "Run the command
        // again so it plans against the project as it is now" is right for a
        // mutation, which would replan; `undo` has one plan and cannot make
        // another. What a reader can do is put the file back or reach for the
        // tool that reverses more than one command.
        if error.code == jails_workspace::PRECONDITION_STALE {
            return Failure::Told(format!(
                "{}\n       fix: put that file back as the last command left it, or use \
                 `git`; `undo` reverses one plan exactly and refuses when the project \
                 has moved under it",
                error.message
            ));
        }
        Failure::diagnosed(
            error.code,
            format!("could not undo the last command: {error}"),
        )
    })?;
    // **Forgotten once it is undone**, so a second `undo` says there is
    // nothing to undo rather than refusing on a stale precondition and
    // leaving the reader to work out which of the two it meant.
    kept.forget();
    if invocation.output == crate::Output::Human {
        println!(
            "undone: {} written, {} deleted",
            execution.files_written, execution.files_deleted
        );
    }
    Ok(())
}
