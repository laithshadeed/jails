//! Executing a canonical test plan through the current engines.

use super::{
    TestOptions, either_root, expand_filter, fingerprint, forced_color, is_maven_program,
    resolve_filter, run_inherited, split_method, test_plan,
};
use jails_protocol::testing::TestReportV1;
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

pub(super) fn warm_report(
    requested: &[String],
    options: &TestOptions,
    debug: bool,
) -> Result<TestReportV1> {
    let timeout = options
        .timeout
        .as_deref()
        .map(test_plan::parse_duration)
        .transpose()?
        .map(std::time::Duration::from_secs);
    if options.affected {
        return crate::testd::affected_report_timeout(debug, timeout);
    }
    crate::testd::run_report_timeout(requested, 0, debug, timeout)
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
    let mut events = once.json.then(|| WatchEvents::new(&root));
    let initial = super::test_report_once(requested, once.clone(), debug)?;
    render_watch_report(&initial, &once, events.as_mut(), 0)?;
    watch_ready(events.as_mut(), 0);
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
            if let Some(events) = events.as_mut() {
                events.emit("overflow", epoch, "{\"action\":\"widen\"}");
            } else {
                println!("test watch: watcher overflow; full rescan complete, widening this run");
            }
            run.affected = false;
        }
        match super::test_report_once(requested, run, debug) {
            Ok(report) => render_watch_report(&report, &once, events.as_mut(), epoch)?,
            Err(error) => match events.as_mut() {
                Some(events) => events.emit(
                    "error",
                    epoch,
                    &format!(
                        "{{\"message\":{}}}",
                        crate::json::string(&error.to_string())
                    ),
                ),
                None => eprintln!("test watch: {error}"),
            },
        }
        watch_ready(events.as_mut(), epoch);
    }
}

fn render_watch_report(
    report: &TestReportV1,
    options: &TestOptions,
    events: Option<&mut WatchEvents>,
    epoch: u64,
) -> Result<()> {
    if let Some(events) = events {
        events.emit("test-report", epoch, &crate::reports::json_line(report));
    } else {
        let _ = crate::reports::render(report, false, options.slowest);
    }
    Ok(())
}

fn watch_ready(events: Option<&mut WatchEvents>, epoch: u64) {
    if let Some(events) = events {
        events.emit("ready", epoch, "{\"output_current\":true}");
    } else {
        println!("test watch: ready; waiting for project changes (Ctrl-C to stop)");
    }
}

struct WatchEvents {
    session: String,
    sequence: u64,
}

impl WatchEvents {
    fn new(root: &Path) -> Self {
        let mut identity = root.to_string_lossy().as_bytes().to_vec();
        identity.extend_from_slice(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_be_bytes(),
        );
        Self {
            session: jails_support::codec::hex(&jails_support::codec::domain_hash(
                "JAILS-TEST-WATCH-SESSION-1",
                &identity,
            )),
            sequence: 0,
        }
    }

    fn emit(&mut self, kind: &str, epoch: u64, data: &str) {
        println!(
            "{}",
            event_line(&self.session, self.sequence, epoch, kind, data)
        );
        self.sequence = self.sequence.saturating_add(1);
    }
}

fn event_line(session: &str, sequence: u64, epoch: u64, kind: &str, data: &str) -> String {
    format!(
        "{{\"schema\":\"jails.event.v1\",\"session\":{},\"sequence\":{},\"epoch\":{},\"kind\":{},\"data\":{}}}",
        crate::json::string(session),
        sequence,
        epoch,
        crate::json::string(kind),
        data
    )
}

pub(super) fn maven_report(
    context: &MavenTestContext<'_>,
    requested: &[String],
) -> Result<TestReportV1> {
    let mut passed = true;
    if requested.is_empty() {
        let mut command = Command::new(crate::maven::binary(context.project));
        match context.options.scope {
            jails_protocol::testing::TestScope::Unit => {
                command.arg("test");
            }
            jails_protocol::testing::TestScope::Integration => {
                command.arg("verify").arg("-Dsurefire.skip=true");
            }
            jails_protocol::testing::TestScope::All => {
                command.arg("verify");
            }
        }
        passed = run_maven_command(context, command).is_ok();
    } else {
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

        if !unit.is_empty() {
            passed &= run_maven_selection(context, "test", "-Dtest", &unit).is_ok();
        }
        if !integration.is_empty() && !(context.options.fail_fast && !passed) {
            passed &= run_maven_selection(context, "verify", "-Dit.test", &integration).is_ok();
        }
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
    run_maven_command(context, cmd)
}

fn run_maven_command(context: &MavenTestContext<'_>, mut cmd: Command) -> Result<()> {
    if context.options.fail_fast {
        cmd.arg("-Dsurefire.skipAfterFailureCount=1")
            .arg("-Dfailsafe.skipAfterFailureCount=1");
    }
    if !context.options.tags.is_empty() {
        cmd.arg(format!("-Dgroups={}", context.options.tags.join(",")));
    }
    cmd.current_dir(context.project);
    if context.options.json {
        if let Some(timeout) = context.options.timeout.as_deref() {
            return run_silent_timeout(
                cmd,
                context.debug,
                std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
            );
        }
        let output = cmd
            .output()
            .map_err(|error| format!("failed to run Maven: {error}"))?;
        return if output.status.success() {
            Ok(())
        } else {
            Err(jails_support::Failure::Reported)
        };
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

pub(super) fn run_silent_timeout(
    mut command: Command,
    debug: bool,
    timeout: std::time::Duration,
) -> Result<()> {
    if debug {
        jails_support::debug_cmd(&command);
    }
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
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
                Err(jails_support::Failure::Reported)
            };
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "test run exceeded {} second(s)\n       fix: raise `--timeout`, narrow the selection, or remove the limit",
                timeout.as_secs()
            )
            .into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn finish_test_report(
    context: &MavenTestContext<'_>,
    requested: &[String],
    passed: bool,
) -> Result<TestReportV1> {
    if !passed && !context.options.json {
        crate::reports::rerun_line(context.project, None);
    }
    crate::reports::normalized(
        context.project,
        jails_protocol::testing::TestEngine::Maven,
        context.options.scope,
        requested,
        passed,
        context.fallback_reason.map(str::to_string),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_events_use_the_editor_protocol_envelope() {
        let line = event_line("session-1", 7, 3, "test-report", "{\"passed\":true}");
        assert_eq!(
            line,
            "{\"schema\":\"jails.event.v1\",\"session\":\"session-1\",\"sequence\":7,\"epoch\":3,\"kind\":\"test-report\",\"data\":{\"passed\":true}}"
        );
        assert!(!line.contains("schema_version"));
    }
}
