//! What this machine's `git` can do, asked once.
//!
//! Only one question so far, and it is the one that cost the most: whether
//! `git merge-file` accepts `--diff-algorithm`. It reached that command after
//! 2.43, so on Ubuntu 24.04 LTS, Debian 12 and RHEL 9 passing it exits **129**
//! -- a usage error, not a merge outcome -- and every regeneration over a file
//! the reader had edited failed. jails passed it unconditionally for months.
//!
//! ## Asked, not guessed from a version
//!
//! `git --version` is a string distributions decorate (`2.39.3 (Apple Git-146)`,
//! `2.43.0.windows.1`), and the number that matters is which release added one
//! flag to one command. So this runs `git merge-file` on three identical
//! throwaway files and reads the exit status. A capability probe cannot be
//! wrong about the thing it just did; a version comparison can.
//!
//! It is the same reason `maven::mvnd_can_start` exists: answer it up front,
//! where the answer is cheap and unambiguous, rather than at a call site where
//! a failure is indistinguishable from the work failing.
//!
//! ## The cost of having a fallback at all, and how it is bounded
//!
//! histogram and myers can resolve an ambiguous merge differently: usually the
//! same bytes, occasionally a different clean result, occasionally a conflict
//! where the other had none. So **two machines can turn one input into two
//! managed trees**, and the accepted projection in `.jails/compiler.lock.json`
//! records whichever they got. That is a real departure from "equal snapshot,
//! patch and compiler version give equal output", and it is why the choice is
//! not only automatic.
//!
//! [`DIFF_ALGORITHM_OVERRIDE`] pins it. A team whose members are on different
//! distributions, or a CI job that must agree with a developer's laptop, sets
//! it once and every machine merges identically -- including to git's default,
//! which is the setting that works everywhere. Without the override the
//! automatic answer is the better merge where it is available and a working
//! one where it is not, which is the trade being made deliberately rather than
//! by accident.

use std::sync::OnceLock;

/// Which diff algorithm `git merge-file` should use, if any.
///
/// Unset asks this machine. Set to a name (`histogram`, `patience`,
/// `minimal`, `myers`) pins that algorithm on every machine. Set to the empty
/// string pins git's own default, which is the one spelling that needs no
/// support from `git merge-file` at all.
pub const DIFF_ALGORITHM_OVERRIDE: &str = "JAILS_GIT_DIFF_ALGORITHM";

/// The algorithm jails prefers when this git can be told.
const PREFERRED: &str = "histogram";

/// The `--diff-algorithm=` argument to pass to `git merge-file`, or `None`.
///
/// Memoised: the probe spawns a process, and the answer cannot change while
/// this one runs.
pub fn merge_diff_algorithm() -> Option<&'static str> {
    static ANSWER: OnceLock<Option<String>> = OnceLock::new();
    ANSWER
        .get_or_init(|| match std::env::var(DIFF_ALGORITHM_OVERRIDE) {
            // Pinned to git's default. The flag is not passed at all, so this
            // is also the setting that works on every git ever shipped.
            Ok(name) if name.trim().is_empty() => None,
            // Pinned to a name. Not validated here: an operator who asks for
            // an algorithm their git does not have should be told by git,
            // naming the flag they set, rather than silently given another.
            Ok(name) => Some(format!("--diff-algorithm={}", name.trim())),
            Err(_) if merge_file_accepts_diff_algorithm() => {
                Some(format!("--diff-algorithm={PREFERRED}"))
            }
            Err(_) => None,
        })
        .as_deref()
}

/// The complete argument list for a `git merge-file` run.
///
/// **One builder for both merge implementations**, because they live in
/// ladders that cannot see each other -- `jails-workspace` is canonical,
/// `jails-prepare` is legacy -- and a capability decision made twice is one
/// that eventually gets made two ways. That is exactly how the flag came to be
/// passed unconditionally from both.
///
/// `flags` is what the caller needs beyond `-p` (conflict-marker style, marker
/// size); `operands` is the labels and the three paths.
pub fn merge_file_argv(flags: &[&str], operands: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut argv = vec!["merge-file".to_string(), "-p".to_string()];
    argv.extend(flags.iter().map(|flag| (*flag).to_string()));
    argv.extend(merge_diff_algorithm().map(str::to_string));
    argv.extend(operands);
    argv
}

/// What to tell a reader whose `git merge-file` run failed, if anything.
///
/// **The one case worth naming is a pin this git cannot honour.** A named
/// [`DIFF_ALGORITHM_OVERRIDE`] is passed through unvalidated -- deliberately,
/// so an operator who asks for an algorithm gets it or gets told -- and "told"
/// has to mean naming the variable they set. Without this the message reads
/// `git merge-file failed as Exited { code: 129 }` and a fix line about
/// verifying git works, which is the shape of refusal this whole mechanism
/// exists to stop shipping.
///
/// `None` when nothing was pinned: then the algorithm is whatever the probe
/// found the machine could do, so the failure is about something else and a
/// guess here would send the reader after the wrong thing.
pub fn pinned_algorithm_hint() -> Option<String> {
    let pinned = std::env::var(DIFF_ALGORITHM_OVERRIDE).ok()?;
    let pinned = pinned.trim();
    (!pinned.is_empty()).then(|| {
        format!(
            "\n       fix: this run pinned `--diff-algorithm={pinned}` from \
             {DIFF_ALGORITHM_OVERRIDE}; git merge-file grew that option after 2.43, so unset it \
             or set {DIFF_ALGORITHM_OVERRIDE}= to use git's default"
        )
    })
}

/// Run `git merge-file` once on three identical files, with the flag.
///
/// Identical inputs so the merge is trivially clean: what is being read is
/// whether the *argument* was understood, and anything non-zero means it was
/// not. A machine with no `git` at all also answers `false` here, and then
/// fails at the real merge with a message about `git` rather than about a
/// flag -- which is the right order to learn those two things in.
fn merge_file_accepts_diff_algorithm() -> bool {
    let Ok(scratch) = crate::scratch::ScratchDir::in_temp("jails-git-probe") else {
        return false;
    };
    for name in ["current", "base", "desired"] {
        if crate::apply::put_in_scratch(scratch.path().join(name), b"probe\n".as_slice()).is_err() {
            return false;
        }
    }
    let invocation = crate::hermetic::Invocation {
        program: "git".into(),
        args: vec![
            "merge-file".into(),
            "-p".into(),
            format!("--diff-algorithm={PREFERRED}"),
            "current".into(),
            "base".into(),
            "desired".into(),
        ],
        working_directory: scratch.path().to_path_buf(),
        environment: crate::hermetic::Invocation::minimal_environment(
            std::env::var("PATH").as_deref().unwrap_or("/usr/bin:/bin"),
            &[],
        ),
        timeout: std::time::Duration::from_secs(10),
    };
    crate::hermetic::run(&invocation).is_ok_and(|run| run.succeeded())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The probe agrees with what `git merge-file` actually does.**
    ///
    /// Written this way because the right answer depends on the machine, so
    /// there is no constant to assert against. What can be asserted is that
    /// the probe and a direct invocation reach the same verdict -- which is
    /// the only property that matters, and the one a version comparison would
    /// eventually get wrong.
    #[test]
    fn the_probe_matches_a_direct_invocation() {
        let probed = merge_file_accepts_diff_algorithm();
        let scratch = crate::scratch::ScratchDir::in_temp("jails-git-probe-check").unwrap();
        for name in ["a", "b", "c"] {
            crate::apply::put_in_scratch(scratch.path().join(name), b"x\n".as_slice()).unwrap();
        }
        let direct = std::process::Command::new("git")
            .current_dir(scratch.path())
            .args([
                "merge-file",
                "-p",
                "--diff-algorithm=histogram",
                "a",
                "b",
                "c",
            ])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        assert_eq!(probed, direct);
    }

    /// **Where the algorithm goes in the argv, on whatever git this is.**
    ///
    /// The value is machine-dependent, so what is asserted is the shape: the
    /// subcommand first, then `-p`, then the caller's flags, then at most one
    /// `--diff-algorithm`, and only then the operands. Put it after the
    /// operands and git reads it as a filename.
    #[test]
    fn the_algorithm_is_spliced_between_the_flags_and_the_operands() {
        let argv = merge_file_argv(
            &["--no-diff3", "--marker-size=7"],
            ["-L".to_string(), "current".to_string(), "base".to_string()],
        );
        assert_eq!(
            &argv[..4],
            ["merge-file", "-p", "--no-diff3", "--marker-size=7"]
        );
        let algorithms = argv
            .iter()
            .filter(|argument| argument.starts_with("--diff-algorithm"))
            .count();
        assert!(algorithms <= 1, "{argv:?}");
        let operands = argv.iter().position(|argument| argument == "-L").unwrap();
        if let Some(at) = argv
            .iter()
            .position(|argument| argument.starts_with("--diff-algorithm"))
        {
            assert!(at > 3 && at < operands, "{argv:?}");
        }
        assert_eq!(&argv[operands..], ["-L", "current", "base"]);
    }

    /// An explicit empty override is "git's default", not "ask the machine".
    ///
    /// Not exercised through [`merge_diff_algorithm`], which memoises for the
    /// life of the process and would make one test decide the answer for
    /// every other in the binary.
    #[test]
    fn an_empty_override_means_no_flag() {
        assert_eq!(resolve(Ok(String::new())), None);
        assert_eq!(resolve(Ok("   ".to_string())), None);
    }

    /// A named override is passed through verbatim, on any git.
    #[test]
    fn a_named_override_is_passed_through() {
        assert_eq!(
            resolve(Ok("patience".to_string())).as_deref(),
            Some("--diff-algorithm=patience")
        );
        // Trimmed, because `JAILS_GIT_DIFF_ALGORITHM=" histogram"` is a typo
        // with an obvious meaning and git would reject the space.
        assert_eq!(
            resolve(Ok(" histogram ".to_string())).as_deref(),
            Some("--diff-algorithm=histogram")
        );
    }

    /// The decision, split out so it can be tested without the memo or the
    /// environment. `merge_diff_algorithm` is this plus a probe and a
    /// `OnceLock`.
    fn resolve(override_value: Result<String, std::env::VarError>) -> Option<String> {
        match override_value {
            Ok(name) if name.trim().is_empty() => None,
            Ok(name) => Some(format!("--diff-algorithm={}", name.trim())),
            Err(_) => None,
        }
    }
}
