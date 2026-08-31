//! Which build tool a directory uses — and, deliberately, nothing more.
//!
//! **The door recognises any build marker, and only the commands that
//! genuinely need Maven refuse — themselves, through [`require_maven`].**
//! Gating the door on `pom.xml` alone refuses about thirty commands on a
//! foreign project, and most of them never touch Maven: `inspect.rs` and
//! `rename.rs` contain zero occurrences of `pom`, and `routes`, `beans`,
//! `stats`, `notes`, `rename`, `destroy --pretend`, `doctor` and most of
//! `generate` read source and write source. `plan.md` §12.
//!
//! ## Reading a Gradle build, and the bar for doing so
//!
//! **jails does read `build.gradle`, and the danger in that is a confident
//! wrong answer** -- a tool that half-understands a build file and reports a
//! dependency the build does not have. Recognising a filename is not
//! understanding a build.
//!
//! So the Gradle reader has one bar: it may only answer questions it can answer
//! exactly, and it must refuse rather than guess. `gradle.rs` states which
//! constructs it understands and returns `None` -- never a default -- for
//! anything else, so a dynamically computed dependency list reads as "cannot
//! tell" rather than as "absent".
//!
//! Not reading it at all is the safer-looking option and the worse one: on a
//! Gradle + Spring Boot project, `add`, `check`, `test`, `build` and `run` all
//! refuse, and `generate` writes code with a note naming the dependencies the
//! reader has to add by hand. Degrading politely is worth less than working
//! when the project is the one you are actually in.
//!
//! A build file jails cannot read at all is still a `Foreign` build, and the
//! cost of that is real and said out loud rather than discovered: generated
//! code is shaped by what the build says, so with nothing readable
//! `repository_wiring` returns `PlainJdbc` and `jspecify_available` is false.
//! Generating into such a project prints which shape it chose. `add` is **not**
//! exempted -- it splices a build file, and a capability that half installs is
//! worse than one that refuses.

use jails_support::Result;
use std::path::Path;

/// What builds this directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// A `pom.xml`. Everything works.
    Maven,
    /// A Groovy `build.gradle` that `gradle::` can read and splice.
    ///
    /// Deliberately *not* every file with `gradle` in the name: a
    /// `build.gradle.kts` is a different language, and a multi-module build
    /// whose root holds only `settings.gradle` declares no dependencies to
    /// read. Both of those stay `Foreign`, because naming a file is not
    /// understanding it and this enum is what decides whether jails will act.
    Gradle,
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
const MARKERS: &[(&str, Marker)] = &[
    ("pom.xml", Marker::Maven),
    ("build.gradle", Marker::Gradle),
    // Kotlin DSL is a different grammar. Recognised as a root so the commands
    // that need no build file still work, and never read.
    ("build.gradle.kts", Marker::Foreign("Gradle")),
    // A multi-module Gradle build puts `build.gradle` in `app/` and only
    // `settings.gradle` at the top, so the settings file has to count as a
    // root or `jails` run from the top finds nothing. It declares no
    // dependencies, so there is nothing for `gradle::` to read.
    ("settings.gradle", Marker::Foreign("Gradle")),
    ("settings.gradle.kts", Marker::Foreign("Gradle")),
    ("build.xml", Marker::Foreign("Ant")),
    ("BUILD.bazel", Marker::Foreign("Bazel")),
];

/// What finding a marker file means.
#[derive(Clone, Copy)]
enum Marker {
    Maven,
    Gradle,
    Foreign(&'static str),
}

impl Build {
    /// What to call it in a message.
    pub fn name(self) -> &'static str {
        match self {
            Build::Maven => "Maven",
            Build::Gradle => "Gradle",
            Build::Foreign(name) => name,
            Build::Bare => "no build tool",
        }
    }
}

/// What builds this directory. Never reads a build file, only names it.
pub fn detect(root: &Path) -> Build {
    for (marker, kind) in MARKERS {
        if root.join(marker).is_file() {
            return match kind {
                Marker::Maven => Build::Maven,
                Marker::Gradle => Build::Gradle,
                Marker::Foreign(name) => Build::Foreign(name),
            };
        }
    }
    Build::Bare
}

/// Whether jails can read and edit this project's build file.
///
/// The one question every "does this work here" decision should ask. It is not
/// the same as "is there a build file": an Ant project has one and jails will
/// not touch it.
pub fn is_readable(build: Build) -> bool {
    matches!(build, Build::Maven | Build::Gradle)
}

/// Refuse a command that cannot work without Maven, and say why.
///
/// The refusal names the command rather than the module, because the reader
/// asked for a command. It also names what *does* work, so the answer is a
/// route forward rather than a wall -- half of jails is useful here.
pub fn require_maven(build: Build, command: &str) -> Result<()> {
    match build {
        Build::Maven | Build::Gradle => Ok(()),
        _ => Err(format!(
            "`jails {command}` needs a build file jails can read, and this one is built by \
             {}.\n       jails reads Maven and Groovy Gradle builds; naming any other file is \
             not understanding it, and a tool that half-understands a build reports \
             dependencies the build does not have.\n       \
             fix: `routes`, `beans`, `stats`, `notes`, `why`, `explain`, `rename`, `doctor` \
             and most of `generate` work here as they are.",
            build.name()
        )
        .into()),
    }
}

/// The same, for a command that has a root but no resolved `Project`.
pub fn require_maven_at(root: &Path, command: &str) -> Result<()> {
    require_maven(detect(root), command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-build-{tag}"))
            .unwrap()
            .keep()
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
