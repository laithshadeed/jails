//! Driving a Gradle build, which is the only thing in this crate that knows
//! how.
//!
//! Apart from `run.rs` by secret rather than by size: that module knows what
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
pub(crate) fn binary(root: &Path) -> PathBuf {
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
/// The report-reading options work here, because the report is the same
/// document: Gradle's `Test` task writes the JUnit XML schema Surefire writes,
/// under `build/test-results/<task>/` instead of `target/*-reports/`.
/// `crate::reports` reads both.
///
/// `--fast` and `--affected` cannot take the warm engine here, and the plan
/// records why rather than ignoring them. Those two need a *resolved
/// classpath*, which jails gets from `dependency:build-classpath`; Gradle has
/// no equivalent without adding a task to a file the reader owns, and doing
/// that for a convenience is a different bargain from splicing a dependency
/// they asked for. A flag that silently ran the whole suite instead would look
/// like it worked -- the fast-path rule from `launcher.rs`, one level up: a
/// fast path falls back *loudly*.
pub(super) fn test_report(
    root: &Path,
    requested: &[String],
    options: &TestOptions,
    fallback_reason: Option<String>,
    debug: bool,
) -> Result<crate::testing::TestReportV1> {
    let build_script = std::fs::read_to_string(root.join("build.gradle")).unwrap_or_default();
    if (!options.tags.is_empty() || options.fail_fast) && !build_script.contains("jails.test.tags")
    {
        return Err(jails_support::Failure::Told(
            "this Gradle build does not expose jails' tag and fail-fast test properties.\n       \
             fix: add the generated `tasks.withType(Test)` contract, or omit `--tags` and \
             `--fail-fast`"
                .to_string(),
        ));
    }

    // Resolved before anything runs, and then followed exactly like a filter
    // the reader typed -- the same shape the Maven path uses.
    let patterns = requested.to_vec();

    let execution_tasks: Vec<&str> = if patterns.is_empty() {
        match options.scope {
            crate::testing::TestScope::Unit => vec!["test"],
            crate::testing::TestScope::Integration => vec!["integrationTest"],
            crate::testing::TestScope::All => vec!["test", "integrationTest"],
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
    if !options.tags.is_empty() {
        selectors.push(format!("-Djails.test.tags={}", options.tags.join(",")));
    }
    if options.fail_fast {
        selectors.push("-Djails.test.failFast=true".into());
    }
    for pattern in &patterns {
        // Gradle's own selector, and repeatable: `--tests` takes a class or
        // method pattern, which is the same shape the reader already types for
        // Surefire.
        selectors.push("--tests".to_string());
        selectors.push(pattern.replace('#', "."));
    }
    let borrowed: Vec<&str> = selectors.iter().map(String::as_str).collect();
    let outcome = match (options.timeout.as_deref(), options.json) {
        (Some(timeout), true) => {
            let mut command = Command::new(binary(root));
            command.args(&borrowed).current_dir(root);
            test_execution::run_silent_timeout(
                command,
                debug,
                std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
            )
        }
        (Some(timeout), false) => {
            let mut command = Command::new(binary(root));
            command.args(&borrowed).current_dir(root);
            test_execution::run_inherited_timeout(
                command,
                debug,
                std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
            )
        }
        (None, true) => {
            let mut command = Command::new(binary(root));
            command.args(&borrowed).current_dir(root);
            if debug {
                jails_support::debug_cmd(&command);
            }
            let output = command.output().map_err(|error| {
                jails_support::Failure::from(format!("failed to run Gradle: {error}"))
            })?;
            if output.status.success() {
                Ok(())
            } else {
                Err(jails_support::Failure::Reported)
            }
        }
        (None, false) => tasks(root, &borrowed, debug),
    };

    // After the run, over the reports it just wrote. `--json` owns the exit
    // status because its whole point is being the machine-readable answer.
    crate::reports::normalized(
        root,
        crate::testing::TestEngine::Gradle,
        options.scope,
        requested,
        outcome.is_ok(),
        fallback_reason,
    )
}
