//! The three-way merge, when the reader and the generator both moved.
//!
//! §R5.3's fifth answer. The other four are decidable from three hashes; this
//! one needs to look at the text, and jails does not implement a merge
//! algorithm -- §R5.4 names `git merge-file`, which is already on the machine
//! of anyone running a code generator and whose conflict grammar people can
//! already read.
//!
//! ## What the two outcomes mean
//!
//! **Clean** is the ordinary case and the one worth having: the reader added a
//! comment to a generated test, the generator added a component, and the two
//! edits do not touch the same lines. Both survive. Without this, `g field` on
//! a project where anybody has ever touched a derivative refuses outright.
//!
//! **Conflicted** is the rare one -- the same lines, differently. §R5.4's
//! answer is marker bytes committed with a frozen `PendingConflict` the next
//! invocation continues or aborts, and that is not wired to a route yet, so
//! this refuses and says so. The bytes are still produced and validated, which
//! is what makes the refusal able to say how many hunks there are.
//!
//! ## Why the inputs are laid out under a hashed key
//!
//! Nothing absolute, random or user-chosen may enter the tool's argv: §R3.3
//! makes the tool fingerprint part of the operation identity, and a scratch
//! path that differs per run would make the same merge a different operation
//! on every machine. `reconcile::path_key` is that name.

use crate::Result;
use crate::reconcile::{MarkerTokens, path_key, validate_conflict};
use jails_protocol::identity::ProjectPath;
use jails_support::hermetic::{self, Invocation, Outcome};
use jails_support::scratch::ScratchDir;
use std::time::Duration;

/// What a three-way merge produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Merged {
    /// The two edits did not overlap. These are the bytes that go on disk;
    /// the recorded *base* still advances to the generator's output, not to
    /// these, so the reader's edit stays a delta from the newest render.
    Clean(Vec<u8>),
    /// The same lines, differently.
    ///
    /// `bytes` is the marker output, kept rather than discarded: §R5.4 commits
    /// it, and a conflict that threw its own resolution away would leave the
    /// reader with nothing to resolve. `tokens` is what git was told to write,
    /// so a resumption can find the regions without re-deriving a convention.
    Conflicted {
        hunks: usize,
        bytes: Vec<u8>,
        tokens: MarkerTokens,
    },
}

/// The label the desired side carries in conflict output.
///
/// §R5.4 derives it from the operation id, which is not computable here: the
/// identity hashes the operations this merge is producing. It is derived from
/// the path instead, which is deterministic, is already the name the inputs
/// are laid out under, and cannot collide across paths in one transaction.
/// The distinction only reaches bytes that are *kept* once markers can be
/// committed -- a clean merge carries no label at all -- so the operation-id
/// spelling lands with the pending protocol that first keeps them.
fn label(key: &str) -> String {
    format!("jails-desired-{}", &key[..12])
}

/// Merge `desired` into `live` against their common `base`.
pub(crate) fn three_way(
    path: &ProjectPath,
    base: &[u8],
    live: &[u8],
    desired: &[u8],
) -> Result<Merged> {
    for (side, bytes) in [("base", base), ("current", live), ("desired", desired)] {
        if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
            return Err(format!(
                "`{path}` has a {side} image that is not UTF-8 text, and a binary three-way \
                 merge is unsupported.\n       fix: resolve it by hand, or destroy and \
                 regenerate."
            )
            .into());
        }
    }

    let key = path_key(path)?;
    let scratch = ScratchDir::in_temp("jails-merge")?;
    // Outside the `project` child a sandboxed tool would see: these are the
    // merge's own inputs, not part of any projected tree.
    let inputs = scratch.path().join("merge-inputs").join(&key);
    for (name, bytes) in [("current", live), ("base", base), ("desired", desired)] {
        jails_support::apply::put_in_scratch(inputs.join(name), bytes)?;
    }

    let tokens = MarkerTokens {
        start: "<<<<<<< current".to_string(),
        separator: "=======".to_string(),
        end: format!(">>>>>>> {}", label(&key)),
    };
    let run = hermetic::run(&Invocation {
        program: "git".into(),
        args: jails_support::git::merge_file_argv(
            &["--no-diff3", "--marker-size=7"],
            [
                "-L".into(),
                "current".into(),
                "-L".into(),
                "base".into(),
                "-L".into(),
                label(&key),
                "current".into(),
                "base".into(),
                "desired".into(),
            ],
        ),
        working_directory: inputs.clone(),
        environment: Invocation::minimal_environment(
            std::env::var("PATH").as_deref().unwrap_or("/usr/bin:/bin"),
            &[],
        ),
        timeout: Duration::from_secs(30),
    });
    let run = match run {
        Ok(run) => run,
        Err(why) => {
            // A missing or unrunnable Git refuses *this path* and nothing
            // else: an unchanged or newly created file must not acquire a
            // dependency on a tool it never needed.
            scratch.close().ok();
            return Err(format!(
                "`{path}` was changed by both you and the generator, and merging it needs \
                 git: {why}\n       fix: install git, or resolve the file by hand."
            )
            .into());
        }
    };
    let merged = run.stdout.bytes.clone();
    let truncated = run.stdout.truncated;
    let outcome = run.outcome.clone();
    scratch.close()?;

    if truncated {
        return Err(
            format!("`{path}` produced more merge output than jails will hold in memory").into(),
        );
    }
    match outcome {
        Outcome::Exited { code: 0 } => Ok(Merged::Clean(merged)),
        // Git reports the conflict count here, truncated at 127.
        Outcome::Exited { code } if (1..=127).contains(&code) => {
            let text = String::from_utf8(merged)
                .map_err(|_| format!("`{path}`: git produced merge output that is not UTF-8"))?;
            let hunks = validate_conflict(&text, &tokens, code)?;
            Ok(Merged::Conflicted {
                hunks,
                bytes: text.into_bytes(),
                tokens,
            })
        }
        other => Err(format!(
            "`{path}`: git merge-file ended as {other:?}, which is neither a clean merge nor \
             conflict output{}",
            jails_support::git::pinned_algorithm_hint().unwrap_or_default()
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> ProjectPath {
        ProjectPath::parse("src/main/java/com/example/Note.java").unwrap()
    }

    fn available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Two edits in different places both survive. This is the whole reason
    /// the merge exists: without it, a generated file anybody has ever
    /// touched can never be evolved again.
    #[test]
    fn edits_that_do_not_overlap_both_survive() {
        if !available() {
            return;
        }
        let base = "one\ntwo\nthree\n";
        let live = "zero\none\ntwo\nthree\n";
        let desired = "one\ntwo\nthree\nfour\n";

        let merged = three_way(
            &path(),
            base.as_bytes(),
            live.as_bytes(),
            desired.as_bytes(),
        )
        .unwrap();

        let Merged::Clean(bytes) = merged else {
            panic!("{merged:?}");
        };
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "zero\none\ntwo\nthree\nfour\n"
        );
    }

    /// The same line, differently, is a conflict rather than a silent pick.
    #[test]
    fn the_same_line_changed_two_ways_is_a_conflict() {
        if !available() {
            return;
        }
        let base = "one\ntwo\nthree\n";
        let live = "one\nMINE\nthree\n";
        let desired = "one\nTHEIRS\nthree\n";

        let merged = three_way(
            &path(),
            base.as_bytes(),
            live.as_bytes(),
            desired.as_bytes(),
        )
        .unwrap();

        let Merged::Conflicted {
            hunks,
            bytes,
            tokens,
        } = merged
        else {
            panic!("the same line changed twice merged cleanly: {merged:?}");
        };
        assert_eq!(hunks, 1);
        // The marker output is kept, not discarded: §R5.4 commits it, and a
        // conflict that threw away its own resolution would leave the reader
        // nothing to resolve.
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains(&tokens.start), "{text}");
        assert!(text.contains(&tokens.separator), "{text}");
        assert!(text.contains(&tokens.end), "{text}");
    }

    /// Binary data is refused rather than merged into nonsense.
    #[test]
    fn a_binary_image_refuses_by_name() {
        let error = three_way(&path(), b"a\0b", b"a\0c", b"a\0d").unwrap_err();
        assert!(
            error.contains("binary three-way merge is unsupported"),
            "{error}"
        );
    }
}
