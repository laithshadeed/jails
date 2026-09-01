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
    // An explicit choice wins over every rule below it. Without one, which
    // Maven runs depends on what happens to be first on `PATH`, and a machine
    // where the daemon cannot start had no way to say so except by editing
    // `PATH` for every command.
    if let Some(chosen) = std::env::var_os(MAVEN_OVERRIDE)
        && !chosen.is_empty()
    {
        return PathBuf::from(chosen);
    }
    let wrapper = root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper;
    }
    if crate::process::on_path(mvnd_binary()) && mvnd_can_start() {
        PathBuf::from(mvnd_binary())
    } else {
        PathBuf::from("mvn")
    }
}

/// Maven without the daemon, for a caller that must not reuse a process.
///
/// The wrapper and the override still win -- a project that ships `mvnw`
/// pins its own Maven, and an explicit `JAILS_MAVEN` is a choice somebody
/// made. What this skips is only the mvnd preference.
pub fn plain(project: &crate::model::Project) -> PathBuf {
    if let Some(chosen) = std::env::var_os(MAVEN_OVERRIDE)
        && !chosen.is_empty()
    {
        return PathBuf::from(chosen);
    }
    let wrapper = project
        .root()
        .join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper;
    }
    PathBuf::from("mvn")
}

/// The environment variable that names the Maven command to run.
pub const MAVEN_OVERRIDE: &str = "JAILS_MAVEN";

/// Whether mvnd could start at all, asked before choosing it.
///
/// mvnd keeps a registry under the Maven user home and writes it *before*
/// Maven runs, so on a machine whose home is read-only it dies with
/// `.m2/mvnd/registry/<version>/registry.bin: Read-only file system` and no
/// build happens. That failure is indistinguishable from a failing build at
/// the call site -- it is a non-zero exit like any other -- so a retry there
/// would re-run a genuinely broken build. It is answerable *here*, cheaply
/// and deterministically: if the registry's nearest existing ancestor is not
/// writable, mvnd cannot start, and plain `mvn` is the honest choice.
fn mvnd_can_start() -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        // Nothing to check against; let the daemon speak for itself.
        return true;
    };
    let mut at = PathBuf::from(home).join(".m2").join("mvnd");
    loop {
        match std::fs::metadata(&at) {
            Ok(metadata) => return !metadata.permissions().readonly(),
            // Not there yet: mvnd would create it, so the question moves up
            // to whether its parent allows that.
            Err(_) => match at.parent() {
                Some(parent) if parent != at => at = parent.to_path_buf(),
                _ => return true,
            },
        }
    }
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

    /// An explicit choice wins over the wrapper and over `PATH`.
    ///
    /// Without one, which Maven runs is decided by whatever happens to be
    /// installed, and a machine whose mvnd cannot start had no way to say so
    /// except by editing `PATH` for every command.
    #[test]
    fn an_explicit_maven_command_wins() {
        let _guard = jails_testkit::hold_cwd();
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-maven-override")
            .unwrap()
            .keep();
        let wrapper = dir.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
        std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
        assert_eq!(binary(&dir), wrapper);

        // SAFETY: the CWD lock serialises the tests in this binary that touch
        // process-global state, and this is one of them.
        unsafe { std::env::set_var(MAVEN_OVERRIDE, "/opt/maven/bin/mvn") };
        assert_eq!(binary(&dir), PathBuf::from("/opt/maven/bin/mvn"));
        unsafe { std::env::remove_var(MAVEN_OVERRIDE) };

        assert_eq!(binary(&dir), wrapper);
        std::fs::remove_dir_all(&dir).ok();
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
        // `an_explicit_maven_command_wins` temporarily mutates JAILS_MAVEN.
        // Hold the same process-global-state lock so the two resolver calls
        // below observe one environment snapshot when tests run in parallel.
        let _guard = jails_testkit::hold_cwd();
        let root = std::env::temp_dir();
        assert_eq!(
            crate::project::maven_command_for_tests(&root),
            binary(&root)
        );
    }
}
