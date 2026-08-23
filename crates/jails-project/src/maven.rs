//! How to invoke *this project's* Maven — and only that.
//!
//! Split from `run.rs` for a layering reason rather than a size one.
//! `project.rs` reports which Maven command a project would use and `add`
//! formats a tree it has just written; both are below the command layer, and
//! both were reaching up into `run.rs` to ask. That back-edge is what made
//! `project`, `run`, `launcher`, `why` and `add` one twelve-module cycle, and a
//! cycle is a boundary that cannot be enforced.
//!
//! It is deliberately *not* part of [`crate::build`], whose whole contract is
//! that it recognises a build file and never invokes one. Choosing a binary and
//! running a goal are the opposite of that promise.
//!
//! The single-resolver rule this preserves is the reason it exists at all:
//! `project.rs` once had its own copy of the mvnd name that was right on
//! Windows while `run.rs`'s was wrong, so `jails about` reported a Maven
//! command `jails test` would not have used.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The name mvnd is installed under. On Windows it ships as `mvnd.cmd`, so
/// probing for a bare `mvnd` there finds nothing and silently falls back to
/// `mvn`.
fn mvnd_binary() -> &'static str {
    if cfg!(windows) { "mvnd.cmd" } else { "mvnd" }
}

/// Prefer the project's wrapper so its Maven version is reproducible. A
/// project without one keeps the fast mvnd/system-Maven fallback.
///
/// The one place this is decided. `project.rs` reports it, `run.rs` executes
/// it, and the two disagreeing is how you get a tool that describes a build it
/// does not run.
pub fn binary(root: &Path) -> PathBuf {
    let wrapper = root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper;
    }
    if crate::process::on_path(mvnd_binary()) {
        PathBuf::from(mvnd_binary())
    } else {
        PathBuf::from("mvn")
    }
}

/// Format a tree jails has just written, best-effort.
///
/// Formatter *wrapping* cannot be predicted from a template, so `add format`
/// runs the real formatter once rather than trying to emit pre-wrapped Java.
/// A machine without Maven simply gets `false` and a note: failing the capability
/// over a cosmetic pass would be worse than an unformatted tree.
pub fn format_quietly(root: &Path) -> bool {
    Command::new(binary(root))
        .args(["-q", "spotless:apply"])
        .current_dir(root)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mvnd ships as `mvnd.cmd` on Windows. `run.rs` probed for a bare
    /// `mvnd` while `project.rs` probed for `mvnd.cmd`, so on Windows the
    /// command `jails about` reported was not the one `jails test` would run
    /// -- and this side would have tried to execute a name not on disk.
    #[test]
    fn the_mvnd_binary_carries_its_platform_extension() {
        if cfg!(windows) {
            assert_eq!(mvnd_binary(), "mvnd.cmd");
        } else {
            assert_eq!(mvnd_binary(), "mvnd");
        }
    }

    /// The wrapper wins over anything on PATH, so a project's pinned Maven
    /// version is what runs.
    #[test]
    fn the_project_wrapper_is_preferred_over_path() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-maven-binary")
            .unwrap()
            .keep();
        // No wrapper: falls back to something on PATH, never to a wrapper path.
        assert!(!binary(&dir).starts_with(&dir));

        let wrapper = dir.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
        std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
        assert_eq!(binary(&dir), wrapper);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `about` must report the command that will actually be executed.
    #[test]
    fn about_and_run_resolve_the_same_maven() {
        let root = std::env::temp_dir();
        assert_eq!(
            crate::project::maven_command_for_tests(&root),
            binary(&root)
        );
    }
}
