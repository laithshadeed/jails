//! Which build tool a directory uses — and, deliberately, nothing more.
//!
//! `plan.md` §12: in `ideas/minicom-public/spring`, **zero of jails' ~30
//! commands worked**, and the whole gate was eleven lines looking for
//! `pom.xml`. Yet `inspect.rs` and `rename.rs` contain zero occurrences of
//! `pom`: `routes`, `beans`, `stats`, `notes`, `rename`, `destroy --pretend`,
//! `doctor` and most of `generate` never needed Maven at all. They were
//! refused by the door, not by anything they do.
//!
//! So the door widens and the commands that genuinely need Maven say so
//! themselves, through [`require_maven`].
//!
//! ## The line this does not cross
//!
//! **jails never reads, writes, parses or invokes `build.gradle`.** That is
//! strictly less than Gradle support and is worth stating in exactly those
//! words, because the failure this prevents is a *confident wrong answer*: a
//! tool that half-understands a build file will report a dependency the build
//! does not have. Recognising a filename is not understanding a build.
//!
//! The cost is real and has to be said out loud rather than discovered:
//! generated code is shaped by what the pom says, so with no pom
//! `repository_wiring` returns `PlainJdbc` and `jspecify_available` is false.
//! Generating into a foreign project therefore prints which shape it chose.
//! `add` is **not** exempted -- it splices a pom, and a capability that half
//! installs is worse than one that refuses.

use crate::Result;
use std::path::Path;

/// What builds this directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Build {
    /// A `pom.xml`. Everything works.
    Maven,
    /// A build jails recognises by filename and will not read.
    Foreign(&'static str),
    /// Java sources and no build file jails knows.
    Bare,
}

/// The files that mark a project root, nearest wins.
///
/// Ordered so that a directory holding both a `pom.xml` and a `build.gradle`
/// -- a migration in progress -- is read as Maven, which is the one of the two
/// jails can actually act on.
const MARKERS: &[(&str, Option<&str>)] = &[
    ("pom.xml", None),
    ("build.gradle", Some("Gradle")),
    ("build.gradle.kts", Some("Gradle")),
    // A multi-module Gradle build puts `build.gradle` in `app/` and only
    // `settings.gradle` at the top, so the settings file has to count as a
    // root or `jails` run from the top finds nothing.
    ("settings.gradle", Some("Gradle")),
    ("settings.gradle.kts", Some("Gradle")),
    ("build.xml", Some("Ant")),
    ("BUILD.bazel", Some("Bazel")),
];

impl Build {
    /// What to call it in a message.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Build::Maven => "Maven",
            Build::Foreign(name) => name,
            Build::Bare => "no build tool",
        }
    }
}

/// What builds this directory. Never reads a build file, only names it.
pub(crate) fn detect(root: &Path) -> Build {
    for (marker, foreign) in MARKERS {
        if root.join(marker).is_file() {
            return match foreign {
                None => Build::Maven,
                Some(name) => Build::Foreign(name),
            };
        }
    }
    Build::Bare
}

/// Refuse a command that cannot work without Maven, and say why.
///
/// The refusal names the command rather than the module, because the reader
/// asked for a command. It also names what *does* work, so the answer is a
/// route forward rather than a wall -- half of jails is useful here.
pub(crate) fn require_maven(build: Build, command: &str) -> Result<()> {
    match build {
        Build::Maven => Ok(()),
        _ => Err(format!(
            "`jails {command}` needs a Maven project, and this one is built by {}.\n       \
             jails never reads, writes, parses or invokes a foreign build file -- naming one \
             is not understanding it.\n       \
             fix: `routes`, `beans`, `stats`, `notes`, `why`, `explain`, `rename`, `doctor` \
             and most of `generate` work here as they are.",
            build.name()
        )),
    }
}

/// The same, for a command that has a root but no resolved `Project`.
pub(crate) fn require_maven_at(root: &Path, command: &str) -> Result<()> {
    require_maven(detect(root), command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("jails-build-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_pom_is_maven_and_a_gradle_file_is_named_but_not_read() {
        let root = scratch("maven");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        assert_eq!(detect(&root), Build::Maven);

        let other = scratch("gradle");
        fs::write(other.join("build.gradle.kts"), "plugins { java }").unwrap();
        assert_eq!(detect(&other), Build::Foreign("Gradle"));
    }

    /// A multi-module Gradle build has no `build.gradle` at the top.
    #[test]
    fn a_settings_file_alone_is_still_a_project_root() {
        let root = scratch("settings");
        fs::write(root.join("settings.gradle"), "include 'app'").unwrap();
        assert_eq!(detect(&root), Build::Foreign("Gradle"));
    }

    /// A migration in progress has both, and Maven is the one jails can act on.
    #[test]
    fn a_directory_with_both_reads_as_maven() {
        let root = scratch("both");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        fs::write(root.join("build.gradle"), "plugins { java }").unwrap();
        assert_eq!(detect(&root), Build::Maven);
    }

    #[test]
    fn nothing_recognised_is_bare_rather_than_a_guess() {
        assert_eq!(detect(&scratch("bare")), Build::Bare);
    }

    /// The refusal has to name a way forward, not just a wall.
    #[test]
    fn require_maven_names_the_build_and_what_still_works() {
        assert!(require_maven(Build::Maven, "test").is_ok());
        let error = require_maven(Build::Foreign("Gradle"), "test").unwrap_err();
        assert!(error.contains("built by Gradle"), "{error}");
        assert!(error.contains("`jails test`"), "{error}");
        assert!(error.contains("routes"), "{error}");
    }
}
