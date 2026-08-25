//! Driving a Gradle build, which is the only thing in this crate that knows
//! how.
//!
//! Split from `run.rs` by secret rather than by size: that module knows what
//! each jails command *means* -- `build` does not run tests, `check` cleans
//! first -- and this one knows how to say it to Gradle. The Maven half stays
//! in `run.rs` because it is entangled with Surefire report parsing, the mvnd
//! probe and the `spring-boot:run` output scan, none of which have a Gradle
//! counterpart.

use super::{TestOptions, run_inherited, test_execution, test_plan};
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
/// The report-reading options work here now, because the report is the same
/// document: Gradle's `Test` task writes the JUnit XML schema Surefire writes,
/// under `build/test-results/<task>/` instead of `target/*-reports/`.
/// `crate::reports` reads both.
///
/// `--fast` and `--affected` still refuse, and by name rather than by being
/// ignored. Those two need a *resolved classpath*, which jails gets from
/// `dependency:build-classpath`; Gradle has no equivalent without adding a
/// task to a file the reader owns, and doing that for a convenience is a
/// different bargain from splicing a dependency they asked for. A flag that
/// silently ran the whole suite instead would look like it worked -- the
/// fast-path rule from `launcher.rs`, one level up: a fast path falls back
/// *loudly*.
pub(super) fn test(
    root: &Path,
    requested: &[String],
    options: TestOptions,
    debug: bool,
) -> Result<()> {
    if options.fast {
        return Err(jails_support::Failure::Told(
            "`jails test --fast` needs a classpath resolved by Maven, and this project is \
             built by Gradle.\n       fix: `jails test` runs the suite here, and `--failed`, \
             `--json` and `--slowest` all work -- they read the JUnit XML Gradle already \
             writes. The flag is refused rather than ignored, because one that silently did \
             something else is worse than one that says it cannot."
                .to_string(),
        ));
    }
    if !options.tags.is_empty() {
        return Err(jails_support::Failure::Told(
            "this Gradle build does not expose JUnit tag selection as a command-line contract.\n       \
             fix: add a tagged `Test` task to the build, or select classes explicitly with \
             `jails test <class>...`"
                .to_string(),
        ));
    }

    // Resolved before anything runs, and then followed exactly like a filter
    // the reader typed -- the same shape the Maven path uses.
    let patterns = requested.to_vec();

    let execution_tasks: Vec<&str> = if patterns.is_empty() {
        match options.scope {
            jails_protocol::testing::TestScope::Unit => vec!["test"],
            jails_protocol::testing::TestScope::Integration => vec!["integrationTest"],
            jails_protocol::testing::TestScope::All => vec!["test", "integrationTest"],
        }
    } else {
        let has_unit = patterns.iter().any(|pattern| {
            let class = pattern.split(['#', '.']).next_back().unwrap_or(pattern);
            !class.ends_with("IT")
        });
        let has_integration = patterns.iter().any(|pattern| {
            let class = pattern.split(['#', '.']).next_back().unwrap_or(pattern);
            class.ends_with("IT")
        });
        match (has_unit, has_integration) {
            (true, true) => vec!["test", "integrationTest"],
            (false, true) => vec!["integrationTest"],
            _ => vec!["test"],
        }
    };
    let mut selectors: Vec<String> = execution_tasks.into_iter().map(str::to_string).collect();
    for pattern in &patterns {
        // Gradle's own selector, and repeatable: `--tests` takes a class or
        // method pattern, which is the same shape the reader already types for
        // Surefire.
        selectors.push("--tests".to_string());
        selectors.push(pattern.to_string());
    }
    let borrowed: Vec<&str> = selectors.iter().map(String::as_str).collect();
    let outcome = match options.timeout.as_deref() {
        Some(timeout) => {
            let mut command = Command::new(binary(root));
            command.args(&borrowed).current_dir(root);
            test_execution::run_inherited_timeout(
                command,
                debug,
                std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
            )
        }
        None => tasks(root, &borrowed, debug),
    };

    // After the run, over the reports it just wrote. `--json` owns the exit
    // status because its whole point is being the machine-readable answer.
    if let Some(count) = options.slowest {
        crate::reports::report_slowest(root, count);
    }
    match options.json {
        true => crate::reports::report_json(root, outcome.is_ok()),
        false => outcome,
    }
}
