//! Application launch policy and foreground lifecycle.

mod classpath;

use crate::model::Project;
use jails_support::Result;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunLauncher {
    Auto,
    Classpath,
    BuildTool,
    Jar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunCompile {
    Auto,
    Ide,
    Build,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunServices {
    Existing,
    Start,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunOptions {
    pub launcher: RunLauncher,
    pub compile: RunCompile,
    pub services: RunServices,
    pub profiles: Vec<String>,
    pub watch: bool,
}

pub(super) fn run(options: RunOptions, args: &[String], debug: bool) -> Result<()> {
    let project = Project::discover()?;
    services(&project, options.services, debug)?;
    let application_args = application_args(&options.profiles, args);

    if options.watch {
        if options.launcher == RunLauncher::BuildTool {
            return super::build_tool_watch(debug);
        }
        return watch(&project, options.compile, &application_args, debug);
    }
    match options.launcher {
        RunLauncher::BuildTool => build_tool_run(&application_args, debug),
        RunLauncher::Jar => run_jar(&project, options.compile, &application_args, debug),
        RunLauncher::Auto | RunLauncher::Classpath => {
            let resolved = classpath::resolve(&project, options.compile, debug)?;
            println!(
                "jails: classpath-resolved; launching {}",
                resolved.main_class
            );
            let command = direct_command(&project, resolved, &application_args)?;
            super::run_watched(command, debug)
        }
    }
}

fn direct_command(
    project: &Project,
    resolved: classpath::Resolved,
    args: &[OsString],
) -> Result<Command> {
    let joined = std::env::join_paths(&resolved.entries)
        .map_err(|error| format!("failed to join runtime classpath: {error}"))?;
    let mut command = Command::new("java");
    command
        .arg("-cp")
        .arg(joined)
        .arg(resolved.main_class)
        .args(args)
        .current_dir(project.root());
    Ok(command)
}

fn watch(project: &Project, compile: RunCompile, args: &[OsString], debug: bool) -> Result<()> {
    let resolved = classpath::resolve(project, compile, debug)?;
    let mut command = direct_command(project, resolved, args)?;
    if debug {
        jails_support::debug_cmd(&command);
    }
    command
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to launch application JVM: {error}"))?;
    println!("jails: process-started; watching project inputs (Ctrl-C to stop)");
    let mut inputs = super::fingerprint::fingerprint(project.root());
    let mut outputs = classpath::output_id(project);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("failed to inspect application JVM: {error}"))?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "application JVM exited with {status}\n       fix: inspect the application output above, then rerun with `--launcher build-tool` for plugin diagnostics"
                )
                .into())
            };
        }
        let current = super::fingerprint::fingerprint(project.root());
        if current.overflowed() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "application watcher overflowed: {}\n       fix: restore readable project inputs and restart the watch session",
                current.gaps().join("; ")
            )
            .into());
        }
        let changes = super::fingerprint::changes_between(&inputs, &current, project.root());
        if !changes.is_empty() {
            inputs = current;
            for change in &changes {
                println!("jails: {change}");
            }
            match compile {
                RunCompile::Auto | RunCompile::Build => {
                    if let Err(error) = classpath::refresh(project, debug) {
                        eprintln!(
                            "jails: compile failed; application remains on its prior output: {error}"
                        );
                    } else {
                        println!("jails: compiled; DevTools owns application restart");
                    }
                }
                RunCompile::None => {
                    println!("jails: stale; waiting for externally compiled output");
                }
                RunCompile::Ide => unreachable!("resolve refuses an unnegotiated IDE epoch"),
            }
        }
        let current_outputs = classpath::output_id(project);
        if current_outputs != outputs {
            outputs = current_outputs;
            println!("jails: restart-observed; compiled output changed");
        }
    }
}

fn application_args(profiles: &[String], args: &[String]) -> Vec<OsString> {
    let mut application_args = Vec::new();
    if !profiles.is_empty() {
        application_args.push(format!("--spring.profiles.active={}", profiles.join(",")).into());
    }
    application_args.extend(args.iter().map(OsString::from));
    application_args
}

fn build_tool_run(args: &[OsString], debug: bool) -> Result<()> {
    let strings = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    super::build_tool_run(false, &strings, debug)
}

fn run_jar(project: &Project, compile: RunCompile, args: &[OsString], debug: bool) -> Result<()> {
    if matches!(compile, RunCompile::Auto | RunCompile::Build) {
        match project.build() {
            crate::build::Build::Maven => {
                let mut package = Command::new(crate::maven::binary(project.root()));
                package
                    .arg("package")
                    .arg("-DskipTests")
                    .current_dir(project.root());
                super::run_inherited(package, debug)?;
            }
            crate::build::Build::Gradle => {
                super::gradlew::tasks(project.root(), &["bootJar"], debug)?
            }
            _ => {}
        }
    }
    let directory = match project.build() {
        crate::build::Build::Maven => project.root().join("target"),
        crate::build::Build::Gradle => project.root().join("build/libs"),
        _ => project.root().join("target"),
    };
    let mut jars = fs::read_dir(&directory)
        .map_err(|_| {
            format!(
                "no packaged artifact under {}\n       fix: use `--compile build` or `--launcher classpath`",
                directory.display()
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jar"))
        .collect::<Vec<_>>();
    jars.sort();
    let jar = jars.pop().ok_or_else(|| {
        "no packaged jar was found\n       fix: use `--compile build` or `--launcher classpath`"
            .to_string()
    })?;
    let mut command = Command::new("java");
    command
        .arg("-jar")
        .arg(jar)
        .args(args)
        .current_dir(project.root());
    super::run_watched(command, debug)
}

fn services(project: &Project, policy: RunServices, debug: bool) -> Result<()> {
    if !crate::compose::exists(project.root()) || policy == RunServices::None {
        return Ok(());
    }
    if policy == RunServices::Start {
        return crate::compose::up(project.root(), &[], debug)
            .then_some(())
            .ok_or_else(|| {
                "declared services did not start\n       fix: run `jails start` and inspect the Compose diagnostic"
                    .into()
            });
    }
    let (program, prefix) = crate::process::compose_program().ok_or_else(|| {
        "declared services cannot be checked because Compose is unavailable\n       fix: install Docker Compose or use `--services none`"
            .to_string()
    })?;
    let declared = compose_output(project, program, prefix, &["config", "--services"], debug)?;
    let running = compose_output(
        project,
        program,
        prefix,
        &["ps", "--services", "--status", "running"],
        debug,
    )?;
    let running = running.lines().collect::<BTreeSet<_>>();
    let missing = declared
        .lines()
        .filter(|service| !running.contains(service))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        println!("jails: service-check; existing declarations are running");
        Ok(())
    } else {
        Err(format!(
            "declared service(s) are not running: {}\n       fix: run `jails start`, or explicitly choose `--services none`",
            missing.join(", ")
        )
        .into())
    }
}

fn compose_output(
    project: &Project,
    program: &str,
    prefix: &[&str],
    args: &[&str],
    debug: bool,
) -> Result<String> {
    let spec = crate::process::CommandSpec::new(program)
        .args(prefix)
        .args(args)
        .current_dir(project.root())
        .output(crate::process::OutputMode::Capture);
    let done = crate::process::run(&spec, crate::process::Diagnostics::from_flag(debug))?;
    if done.status.success() {
        Ok(done.stdout_string())
    } else {
        Err("Compose service inspection failed\n       fix: run `jails start` and inspect the Compose diagnostic".into())
    }
}
