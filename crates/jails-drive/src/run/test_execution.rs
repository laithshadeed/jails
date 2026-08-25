//! Executing a canonical test plan through the current engines.

use super::{
    TestOptions, either_root, expand_filter, fingerprint, forced_color, is_maven_program,
    resolve_filter, run_inherited, split_method, test_once, test_once_with_fallback, test_plan,
};
use jails_support::Result;
use std::path::Path;
use std::process::Command;

pub(super) struct MavenTestContext<'run> {
    pub project: &'run Path,
    pub options: &'run TestOptions,
    pub fallback_reason: Option<&'run str>,
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
    let (root, _) = either_root("test --engine warm")?;
    if options.compile == jails_protocol::testing::TestCompilePolicy::Ide {
        return delegate_or_refuse(
            requested,
            options,
            debug,
            "no current negotiated IDE output epoch is available".into(),
        );
    }
    let ineligible = super::isolation::refusals(&root, requested);
    if !ineligible.is_empty() {
        return delegate_or_refuse(requested, options, debug, ineligible.join("; "));
    }
    if options.scope != jails_protocol::testing::TestScope::Unit || options.database_schema {
        return delegate_or_refuse(
            requested,
            options,
            debug,
            "the warm engine only accepts isolated unit tests".into(),
        );
    }
    if !options.tags.is_empty() {
        return delegate_or_refuse(
            requested,
            options,
            debug,
            "the warm engine cannot prove JUnit tag eligibility yet".into(),
        );
    }
    if options.timeout.is_some() {
        return delegate_or_refuse(
            requested,
            options,
            debug,
            "testd v2 cancellation is not active for timed requests".into(),
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
    crate::reports::render(&report, options.json, options.slowest)
}

fn delegate_or_refuse(
    requested: &[String],
    options: &TestOptions,
    debug: bool,
    reason: String,
) -> Result<()> {
    if options.engine == jails_protocol::testing::TestEnginePolicy::Auto {
        if options.compile == jails_protocol::testing::TestCompilePolicy::None {
            return Err(format!(
                "automatic warm execution is ineligible: {reason}\n       fix: compile explicitly, or choose `--compile auto` so the build tool may own this partition"
            )
            .into());
        }
        if options.compile == jails_protocol::testing::TestCompilePolicy::Ide {
            return Err(format!(
                "automatic warm execution is ineligible: {reason}; no current negotiated IDE output epoch is available\n       fix: connect an editor epoch, or choose `--compile auto`"
            )
            .into());
        }
        println!("test engine delegated to the build tool: {reason}");
        let mut delegated = options.clone();
        delegated.engine = jails_protocol::testing::TestEnginePolicy::Build;
        delegated.compile = jails_protocol::testing::TestCompilePolicy::Build;
        delegated.fast = false;
        return test_once_with_fallback(requested, delegated, debug, Some(reason));
    }
    Err(format!(
        "strict warm execution is ineligible: {reason}\n       fix: choose `--engine auto` so the build tool owns this partition"
    )
    .into())
}

pub(super) fn test_watch(requested: &[String], options: TestOptions, debug: bool) -> Result<()> {
    let (root, _) = either_root("test --watch")?;
    let mut once = options;
    once.watch = false;
    once.repeat = 1;
    once.until_fail = false;
    let mut previous = fingerprint::fingerprint(&root);
    if previous.overflowed() {
        return Err(format!(
            "test watch cannot establish its initial snapshot: {}\n       fix: restore readable project inputs and retry",
            previous.gaps().join("; ")
        )
        .into());
    }
    test_once(requested, once.clone(), debug)?;
    watch_ready(once.json, 0);
    let mut pending = super::watch::Batch::default();
    let mut epoch = 0_u64;
    loop {
        std::thread::sleep(super::watch::POLL);
        let current = fingerprint::fingerprint(&root);
        let changes = fingerprint::changes_between(&previous, &current, &root);
        let overflow = current.overflowed();
        if !changes.is_empty() || overflow {
            previous = current;
            pending.observe(std::time::Instant::now(), changes, overflow);
        }
        if !pending.due(std::time::Instant::now()) {
            continue;
        }
        if previous.overflowed() {
            continue;
        }
        let (changes, overflowed) = pending.take();
        epoch = epoch.saturating_add(1);
        if !once.json {
            println!();
            for change in &changes {
                println!("test watch: {change}");
            }
        }
        let mut run = once.clone();
        if overflowed {
            if once.json {
                println!("{{\"schema_version\":1,\"event\":\"overflow\",\"action\":\"widen\"}}");
            } else {
                println!("test watch: watcher overflow; full rescan complete, widening this run");
            }
            run.affected = false;
        }
        if let Err(error) = test_once(requested, run, debug) {
            if once.json {
                println!(
                    "{{\"schema_version\":1,\"event\":\"error\",\"message\":{}}}",
                    crate::json::string(&error.to_string())
                );
            } else {
                eprintln!("test watch: {error}");
            }
        }
        watch_ready(once.json, epoch);
    }
}

fn watch_ready(json: bool, epoch: u64) {
    if json {
        println!(
            "{{\"schema_version\":1,\"event\":\"ready\",\"epoch\":{epoch},\"output_current\":true}}"
        );
    } else {
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
    finish_test_report(context, requested, passed)
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

fn finish_test_report(
    context: &MavenTestContext<'_>,
    requested: &[String],
    passed: bool,
) -> Result<()> {
    if !passed && !context.options.json {
        crate::reports::rerun_line(context.project, None);
    }
    let report = crate::reports::normalized(
        context.project,
        jails_protocol::testing::TestEngine::Maven,
        context.options.scope,
        requested,
        passed,
        context.fallback_reason.map(str::to_string),
    )?;
    crate::reports::render(&report, context.options.json, context.options.slowest)
}
