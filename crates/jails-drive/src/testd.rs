//! Resident JVM execution behind the canonical `jails test` coordinator.
//!
//! `jails testd` remains a compatibility alias, but there is only one engine:
//! the authenticated v2 daemon in this module. It consumes compiled output and
//! never compiles, starts infrastructure, or attaches to an application JVM.

mod v2;

use crate::affected;
use crate::build;
use crate::launcher;
use crate::model::Project;
use jails_protocol::testing::TestReportV1;
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
    let client = v2::Client::for_project(&project)?;
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
            render(run_report_in(&project, &requested, 0, debug)?)
        }
        Action::Affected => run_affected(&project, debug),
    }
}

pub(crate) fn run_report(requested: &[String], epoch: u64, debug: bool) -> Result<TestReportV1> {
    let project = Project::discover()?;
    build::require_maven(project.build(), "test --engine warm")?;
    run_report_in(&project, requested, epoch, debug)
}

fn run_report_in(
    project: &Project,
    requested: &[String],
    epoch: u64,
    debug: bool,
) -> Result<TestReportV1> {
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
    let client = v2::Client::for_project(project)?;
    client.ensure_running(project, &classpath, debug)?;
    client.run(&classpath, &selectors, epoch)
}

fn run_affected(project: &Project, debug: bool) -> Result<()> {
    match affected::select(project.root(), debug) {
        affected::Selection::Nothing { epoch } => {
            println!("testd: no affected tests in epoch {epoch}");
            Ok(())
        }
        affected::Selection::Everything { epoch, reasons } => {
            println!("testd: running everything -- {}", reasons.join("; "));
            render(run_report_in(project, &[], epoch, debug)?)
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
            render(run_report_in(project, &tests, epoch, debug)?)
        }
    }
}

pub(crate) fn render(report: TestReportV1) -> Result<()> {
    for case in &report.cases {
        if !case.stdout_summary.is_empty() {
            print!("{}", case.stdout_summary);
        }
        if !case.stderr_summary.is_empty() {
            eprint!("{}", case.stderr_summary);
        }
    }
    if report.succeeded() {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rendered_daemon_source_has_no_unresolved_protocol_tokens() {
        let source = super::v2::rendered_daemon_source();
        assert!(source.contains("PROTOCOL_MIN = 2"));
        assert!(!source.contains("@JAILS_TESTD_"));
    }

    #[test]
    fn daemon_source_keeps_the_bounded_recycle_controls() {
        let source = super::v2::rendered_daemon_source();
        assert!(source.contains("MAX_GENERATIONS = 50"));
        assert!(source.contains("128L * 1024L * 1024L"));
        assert!(source.contains("leakedThread()"));
    }
}
