//! Executing a canonical test plan through the current engines.

use super::{
    TestOptions, either_root, expand_filter, fingerprint, forced_color, is_maven_program,
    report_rerun_line, resolve_filter, run_inherited, split_method, test_once, test_plan,
};
use jails_support::Result;
use std::path::Path;
use std::process::Command;

pub(super) struct MavenTestContext<'run> {
    pub project: &'run Path,
    pub options: &'run TestOptions,
    pub debug: bool,
}

pub(super) fn run_inherited_timeout(
    mut cmd: Command,
    debug: bool,
    timeout: std::time::Duration,
) -> Result<()> {
    if is_maven_program(cmd.get_program()) {
        forced_color(&mut cmd);
    }
    if debug {
        jails_support::debug_cmd(&cmd);
    }
    cmd.stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .map_err(|error| format!("failed to run {program}: {error}"))?;
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("failed to wait for {program}: {error}"))?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "{program} exited with {status}\n       fix: inspect the test output above, then \
                     rerun the failing selector"
                )
                .into())
            };
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "test run exceeded {} second(s)\n       fix: raise `--timeout`, narrow the \
                 selection, or remove the limit",
                timeout.as_secs()
            )
            .into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub(super) fn run_warm(requested: &[String], options: &TestOptions, debug: bool) -> Result<()> {
    if options.scope != jails_protocol::testing::TestScope::Unit || options.database_schema {
        return Err(
            "the warm engine only accepts isolated unit tests\n       fix: choose `--engine auto` \
             so integration tests delegate to the build tool"
                .into(),
        );
    }
    if !options.tags.is_empty() {
        return Err(
            "the warm engine cannot prove JUnit tag eligibility yet\n       fix: choose `--engine \
             auto` so tagged selection delegates safely"
                .into(),
        );
    }
    if options.timeout.is_some() {
        return Err(
            "the v1 warm transport cannot cancel a timed-out request\n       fix: choose `--engine \
             build` until testd v2 cancellation is active"
                .into(),
        );
    }
    if options.affected {
        if !requested.is_empty() {
            return Err(
                "explicit selectors cannot yet be intersected with the warm affected graph\n       \
                 fix: use either selectors or `--affected`, or choose `--engine build`"
                    .into(),
            );
        }
        return crate::testd::testd(crate::testd::Action::Affected, debug);
    }
    let report = crate::testd::run_report(requested, 0, debug)?;
    crate::testd::render(report)
}

pub(super) fn test_watch(requested: &[String], options: TestOptions, debug: bool) -> Result<()> {
    let (root, _) = either_root("test --watch")?;
    let mut once = options;
    once.watch = false;
    once.repeat = 1;
    once.until_fail = false;
    test_once(requested, once.clone(), debug)?;
    let mut previous = fingerprint::fingerprint(&root);
    println!("test watch: ready; waiting for project changes (Ctrl-C to stop)");
    loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let current = fingerprint::fingerprint(&root);
        let changes = fingerprint::changes_between(&previous, &current, &root);
        if changes.is_empty() {
            continue;
        }
        previous = current;
        println!();
        for change in &changes {
            println!("test watch: {change}");
        }
        if let Err(error) = test_once(requested, once.clone(), debug) {
            eprintln!("test watch: {error}");
        }
        println!("test watch: ready; waiting for project changes (Ctrl-C to stop)");
    }
}

pub(super) fn test_many_maven(context: &MavenTestContext<'_>, requested: &[String]) -> Result<()> {
    if context.options.fast
        || context.options.engine == jails_protocol::testing::TestEnginePolicy::Warm
    {
        for selector in requested {
            let mut one = context.options.clone();
            one.failed = false;
            one.repeat = 1;
            test_once(std::slice::from_ref(selector), one, context.debug)?;
        }
        return Ok(());
    }

    let mut unit = Vec::new();
    let mut integration = Vec::new();
    for filter in requested {
        let resolved = resolve_filter(context.project, filter)?;
        let expanded = expand_filter(&resolved);
        let (class, _) = split_method(&expanded);
        if class.ends_with("IT") {
            integration.push(expanded);
        } else {
            unit.push(expanded);
        }
    }

    let mut passed = true;
    if !unit.is_empty() {
        passed &= run_maven_selection(context, "test", "-Dtest", &unit).is_ok();
    }
    if !integration.is_empty() {
        passed &= run_maven_selection(context, "verify", "-Dit.test", &integration).is_ok();
    }
    finish_test_report(context, passed)
}

fn run_maven_selection(
    context: &MavenTestContext<'_>,
    phase: &str,
    property: &str,
    selectors: &[String],
) -> Result<()> {
    let mut cmd = Command::new(crate::maven::binary(context.project));
    cmd.arg(phase)
        .arg(format!("{property}={}", selectors.join(",")))
        .arg("-Dsurefire.failIfNoSpecifiedTests=false")
        .arg("-Dfailsafe.failIfNoSpecifiedTests=false")
        .current_dir(context.project);
    if context.options.fail_fast {
        cmd.arg("-Dsurefire.skipAfterFailureCount=1")
            .arg("-Dfailsafe.skipAfterFailureCount=1");
    }
    if !context.options.tags.is_empty() {
        cmd.arg(format!("-Dgroups={}", context.options.tags.join(",")));
    }
    match context.options.timeout.as_deref() {
        Some(timeout) => run_inherited_timeout(
            cmd,
            context.debug,
            std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
        ),
        None => run_inherited(cmd, context.debug),
    }
}

fn finish_test_report(context: &MavenTestContext<'_>, passed: bool) -> Result<()> {
    if let Some(count) = context.options.slowest {
        crate::reports::report_slowest(context.project, count);
    }
    if context.options.json {
        crate::reports::report_json(context.project, passed)
    } else if passed {
        Ok(())
    } else {
        report_rerun_line(context.project, None);
        Err(jails_support::Failure::Reported)
    }
}
