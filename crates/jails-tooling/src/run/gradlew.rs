//! Driving a Gradle build, which is the only thing in this crate that knows
//! how.
//!
//! Split from `run.rs` by secret rather than by size: that module knows what
//! each jails command *means* -- `build` does not run tests, `check` cleans
//! first -- and this one knows how to say it to Gradle. The Maven half stays
//! in `run.rs` because it is entangled with Surefire report parsing, the mvnd
//! probe and the `spring-boot:run` output scan, none of which have a Gradle
//! counterpart.

use super::{TestOptions, run_inherited};
use jails_support::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Gradle command to invoke: the project's wrapper when it has one.
///
/// The wrapper is strongly preferred and not merely first: Gradle pins its own
/// version in `gradle/wrapper/gradle-wrapper.properties`, so `gradle` off PATH
/// can be a different major than the build was written for. Same reasoning as
/// `maven::binary` preferring `./mvnw`.
pub(super) fn binary(root: &Path) -> PathBuf {
    let wrapper = root.join("gradlew");
    match wrapper.is_file() {
        true => wrapper,
        false => PathBuf::from("gradle"),
    }
}

/// One Gradle invocation, from the project root.
pub(super) fn tasks(root: &Path, names: &[&str], debug: bool) -> Result<()> {
    let mut cmd = Command::new(binary(root));
    cmd.args(names).current_dir(root);
    run_inherited(cmd, debug)
}

/// `jails test` on a Gradle build.
///
/// The plain case only, and the options it cannot honour refuse by name rather
/// than being ignored. `--fast`, `--affected` and `--failed` are all built on
/// reading Maven's own output -- the Surefire report directory, a classpath
/// resolved by `dependency:build-classpath` -- and silently downgrading to a
/// full run would make the flag look like it worked while doing something
/// else. That is the fast-path rule from `launcher.rs` applied one level up:
/// a fast path falls back *loudly*.
pub(super) fn test(
    root: &Path,
    filter: Option<&str>,
    options: TestOptions,
    debug: bool,
) -> Result<()> {
    for (asked, flag) in [
        (options.fast, "--fast"),
        (options.failed, "--failed"),
        (options.json, "--json"),
        (options.slowest.is_some(), "--slowest"),
    ] {
        if asked {
            return Err(format!(
                "`jails test {flag}` reads Maven's own output -- the Surefire reports, or a \
                 classpath resolved through Maven -- and this project is built by \
                 Gradle.\n       fix: `jails test` runs the suite here, and `--tests` \
                 patterns work through the filter argument. The flag is refused rather than \
                 ignored, because one that silently did something else is worse than one \
                 that says it cannot."
            ));
        }
    }
    let mut selectors = vec!["test".to_string()];
    if let Some(filter) = filter {
        // Gradle's own selector. `--tests` takes a class or method pattern,
        // which is the same shape the reader already types for Surefire.
        selectors.push("--tests".to_string());
        selectors.push(filter.to_string());
    }
    let borrowed: Vec<&str> = selectors.iter().map(String::as_str).collect();
    tasks(root, &borrowed, debug)
}
