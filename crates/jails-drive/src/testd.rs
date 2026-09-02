//! Resident JVM execution behind the canonical `jails test` coordinator.
//!
//! `jails testd` remains a compatibility alias, but there is only one engine:
//! the authenticated daemon in this module. It consumes compiled output and
//! never compiles, starts infrastructure, or attaches to an application JVM.
//!
//! Two files, split by what each knows: `protocol` is the frames, `client` is
//! the process lifecycle and the one place a daemon observation becomes a
//! `crate::testing::TestReport`.

mod client;
mod protocol;

use crate::affected;
use crate::build;
use crate::launcher;
use crate::model::Project;
use crate::testing::TestReport;
use jails_support::Result;

pub enum Action {
    Run(Option<String>),
    Affected,
    Stop,
    Status,
    Restart,
}

pub fn testd(action: Action, debug: bool) -> Result<()> {
    let project = Project::discover()?;
    build::require_maven(project.build(), "testd")?;
    let client = client::Client::for_project(&project)?;
    match action {
        Action::Stop => client.stop(),
        Action::Status => client.status(),
        Action::Restart => {
            client.stop_quietly();
            let classpath = launcher::test_classpath(project.root(), debug)?;
            client.ensure_running(&project, &classpath, debug)?;
            println!("testd: running ({})", client.socket().display());
            Ok(())
        }
        Action::Run(filter) => {
            let requested = filter.into_iter().collect::<Vec<_>>();
            render(run_report_in(&project, &requested, 0, debug, None)?)
        }
        Action::Affected => render(affected_report_in(&project, debug, None)?),
    }
}

pub(crate) fn run_report_timeout(
    requested: &[String],
    epoch: u64,
    debug: bool,
    timeout: Option<std::time::Duration>,
) -> Result<TestReport> {
    let project = Project::discover()?;
    build::require_maven(project.build(), "test --engine warm")?;
    run_report_in(&project, requested, epoch, debug, timeout)
}

fn run_report_in(
    project: &Project,
    requested: &[String],
    epoch: u64,
    debug: bool,
    timeout: Option<std::time::Duration>,
) -> Result<TestReport> {
    if let Some(stale) = launcher::staleness(project.root()) {
        return Err(format!(
            "testd not taken: {}\n       fix: compile through `jails test --engine build` and retry",
            stale.explain()
        )
        .into());
    }
    let selectors = requested
        .iter()
        .map(|selector| {
            launcher::fully_qualified(project.root(), selector).ok_or_else(|| {
                format!(
                    "testd not taken: could not resolve `{selector}` to a fully qualified name\n       \
                     fix: pass the qualified class name or choose `--engine build`"
                )
                .into()
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let classpath = launcher::test_classpath(project.root(), debug)?;
    let client = client::Client::for_project(project)?;
    client.ensure_running(project, &classpath, debug)?;
    client.run(&classpath, &selectors, epoch, timeout)
}

pub(crate) fn affected_report_timeout(
    debug: bool,
    timeout: Option<std::time::Duration>,
) -> Result<TestReport> {
    let project = Project::discover()?;
    build::require_maven(project.build(), "test --affected")?;
    affected_report_in(&project, debug, timeout)
}

fn affected_report_in(
    project: &Project,
    debug: bool,
    timeout: Option<std::time::Duration>,
) -> Result<TestReport> {
    match affected::select(project.root(), debug) {
        affected::Selection::Nothing { epoch } => {
            println!("testd: no affected tests in epoch {epoch}");
            Ok(TestReport {
                epoch,
                passed: true,
                scope: crate::testing::TestScope::Unit,
                requested: Vec::new(),
                cases: Vec::new(),
                fallback_reasons: Vec::new(),
            })
        }
        affected::Selection::Everything { epoch, reasons } => {
            println!("testd: running everything -- {}", reasons.join("; "));
            let mut report = run_report_in(project, &[], epoch, debug, timeout)?;
            report.fallback_reasons.extend(reasons);
            Ok(report)
        }
        affected::Selection::Stale { epoch, reasons } => Err(format!(
            "testd epoch {epoch} is not runnable: {}\n       fix: compile through `jails test --engine build` and retry",
            reasons.join("; ")
        )
        .into()),
        affected::Selection::Tests { epoch, tests } => {
            println!(
                "testd: {} test class(es) reachable from the working tree's changes",
                tests.len()
            );
            run_report_in(project, &tests, epoch, debug, timeout)
        }
    }
}

pub(crate) fn render(report: TestReport) -> Result<()> {
    crate::reports::render(&report, false, None)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rendered_daemon_source_has_no_unresolved_protocol_tokens() {
        let source = super::client::rendered_daemon_source();
        assert!(source.contains("PROTOCOL_MIN = 3"));
        assert!(!source.contains("@JAILS_TESTD_"));
    }

    #[test]
    fn daemon_source_keeps_the_bounded_recycle_controls() {
        let source = super::client::rendered_daemon_source();
        assert!(source.contains("MAX_GENERATIONS = 50"));
        assert!(source.contains("128L * 1024L * 1024L"));
        assert!(source.contains("leakedThread()"));
    }
}
