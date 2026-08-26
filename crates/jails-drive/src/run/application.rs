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
            let java = java_executable(&project, debug)?;
            println!(
                "jails: classpath-resolved; launching {}",
                resolved.main_class
            );
            let command = direct_command(&project, &java, resolved, &application_args)?;
            super::run_watched(command, debug)
        }
    }
}

fn direct_command(
    project: &Project,
    java: &std::path::Path,
    resolved: classpath::Resolved,
    args: &[OsString],
) -> Result<Command> {
    let joined = std::env::join_paths(&resolved.entries)
        .map_err(|error| format!("failed to join runtime classpath: {error}"))?;
    let mut command = Command::new(java);
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
    let java = java_executable(project, debug)?;
    let mut command = direct_command(project, &java, resolved, args)?;
    if debug {
        jails_support::debug_cmd(&command);
    }
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    jails_support::hermetic::own_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to launch application JVM: {error}"))?;
    let _signals = match jails_support::hermetic::ForegroundSignals::install(child.id()) {
        Ok(signals) => signals,
        Err(error) => {
            let _ = jails_support::hermetic::terminate_process_group(&mut child);
            return Err(error);
        }
    };
    println!(
        "jails: process-started; pid={}; watching project inputs (Ctrl-C to stop)",
        child.id()
    );
    let stdout = child.stdout.take();
    let output = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut lifecycle = super::ApplicationSignals::default();
        let mut log = String::new();
        if let Some(mut stdout) = stdout {
            let mut chunk = [0_u8; 4096];
            while let Ok(read) = stdout.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&chunk[..read]);
                print!("{text}");
                std::io::Write::flush(&mut std::io::stdout()).ok();
                log.push_str(&text);
                lifecycle.observe(&log);
            }
        }
    });
    let mut inputs = super::fingerprint::fingerprint(project.root());
    let mut outputs = classpath::output_id(project);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("failed to inspect application JVM: {error}"))?
        {
            let _ = output.join();
            println!("jails: stopped; status={status}");
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
            jails_support::hermetic::terminate_process_group(&mut child)?;
            let _ = output.join();
            println!("jails: stopped; reason=watcher-overflow");
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

fn java_executable(project: &Project, debug: bool) -> Result<std::path::PathBuf> {
    let java = std::env::var_os("JAVA_HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("bin/java"))
        .unwrap_or_else(|| std::path::PathBuf::from("java"));
    let output = Command::new(&java)
        .arg("-version")
        .output()
        .map_err(|error| {
            format!(
                "selected Java executable `{}` is unavailable: {error}\n       fix: set JAVA_HOME to a JDK that supports this project",
                java.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "selected Java executable `{}` rejected `-version`\n       fix: set JAVA_HOME to a working JDK",
            java.display()
        )
        .into());
    }
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(required) = project.java_release() {
        let actual = java_major(&version).ok_or_else(|| {
            format!(
                "could not read the selected Java release from `{}`\n       fix: set JAVA_HOME to an ordinary JDK {required}+ installation",
                java.display()
            )
        })?;
        if actual < required {
            return Err(format!(
                "selected Java {actual} cannot run project release {required}\n       fix: set JAVA_HOME to JDK {required} or newer"
            )
            .into());
        }
        if debug {
            eprintln!(
                "jails: selected Java {} supports project release {required}",
                java.display()
            );
        }
    }
    Ok(java)
}

fn java_major(version: &str) -> Option<u32> {
    let version = version
        .split_once('"')
        .map(|(_, rest)| rest.split('"').next().unwrap_or(rest))
        .or_else(|| {
            version.split_whitespace().find(|part| {
                part.chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_digit())
            })
        })?;
    let first = version.split('.').next()?.parse::<u32>().ok()?;
    if first == 1 {
        version.split('.').nth(1)?.parse().ok()
    } else {
        Some(first)
    }
}

fn build_tool_run(args: &[OsString], debug: bool) -> Result<()> {
    let strings = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    super::build_tool_run(false, &strings, debug)
}

fn run_jar(project: &Project, compile: RunCompile, args: &[OsString], debug: bool) -> Result<()> {
    if compile == RunCompile::Ide {
        return Err(
            "`--compile ide` requires a negotiated editor output epoch\n       fix: connect the editor session, or use `--compile auto`"
                .into(),
        );
    }
    let built = matches!(compile, RunCompile::Auto | RunCompile::Build);
    if built {
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
        .filter(|path| is_executable_jar_candidate(path))
        .collect::<Vec<_>>();
    jars.sort();
    let jar = match jars.as_slice() {
        [] => {
            return Err(
                "no executable packaged jar was found\n       fix: use `--compile build` or `--launcher classpath`"
                    .into(),
            );
        }
        [jar] => jar.clone(),
        _ => {
            return Err(format!(
                "packaged artifact is ambiguous: {}\n       fix: remove stale executable jars or choose `--launcher classpath`",
                jars.iter()
                    .map(|path| path.file_name().unwrap_or_default().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
    };
    if built {
        write_artifact_proof(project, &jar)?;
    } else {
        verify_artifact_proof(project, &jar)?;
    }
    let java = java_executable(project, debug)?;
    let mut command = Command::new(java);
    command
        .arg("-jar")
        .arg(jar)
        .args(args)
        .current_dir(project.root());
    super::run_watched(command, debug)
}

fn is_executable_jar_candidate(path: &std::path::Path) -> bool {
    if path.extension().is_none_or(|extension| extension != "jar") {
        return false;
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    !["-plain.jar", "-sources.jar", "-javadoc.jar"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

const ARTIFACT_PROOF: &str = "packaged-artifact-v1";

fn write_artifact_proof(project: &Project, jar: &std::path::Path) -> Result<()> {
    let relative = jar.strip_prefix(project.root()).map_err(|_| {
        "packaged artifact escapes the project\n       fix: rebuild into the ordinary target directory"
    })?;
    let text = format!(
        "version=1\nroot={}\ninputs={}\npath={}\nartifact={}\n",
        project_root_id(project),
        artifact_inputs(project),
        relative.to_string_lossy(),
        file_digest(jar)?
    );
    let proof = project.root().join(".jails/run").join(ARTIFACT_PROOF);
    jails_support::apply::put_runtime_state(project.root(), &proof, text.as_bytes())
}

fn verify_artifact_proof(project: &Project, jar: &std::path::Path) -> Result<()> {
    let proof = project.root().join(".jails/run").join(ARTIFACT_PROOF);
    let text = fs::read_to_string(&proof).map_err(|_| {
        "packaged artifact has no current jails proof\n       fix: run `jails run --launcher jar --compile build` once"
    })?;
    let expected_path = jar
        .strip_prefix(project.root())
        .unwrap_or(jar)
        .to_string_lossy();
    let expected = [
        "version=1".to_string(),
        format!("root={}", project_root_id(project)),
        format!("inputs={}", artifact_inputs(project)),
        format!("path={expected_path}"),
        format!("artifact={}", file_digest(jar)?),
    ];
    if text.lines().eq(expected.iter().map(String::as_str)) {
        Ok(())
    } else {
        Err(
            "packaged artifact is stale or belongs to different inputs\n       fix: run `jails run --launcher jar --compile build`"
                .into(),
        )
    }
}

fn artifact_inputs(project: &Project) -> String {
    let mut files = [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "gradle.properties",
        "gradle/libs.versions.toml",
        "gradle/wrapper/gradle-wrapper.properties",
        ".mvn/maven.config",
        ".mvn/wrapper/maven-wrapper.properties",
    ]
    .into_iter()
    .map(|path| project.root().join(path))
    .filter(|path| path.is_file())
    .collect::<Vec<_>>();
    files.extend(project_input_files(project));
    files.sort();
    files.dedup();
    let mut bytes = Vec::new();
    for file in files {
        bytes.extend_from_slice(
            file.strip_prefix(project.root())
                .unwrap_or(&file)
                .to_string_lossy()
                .as_bytes(),
        );
        bytes.push(0);
        if let Ok(content) = fs::read(file) {
            bytes.extend_from_slice(&jails_support::codec::domain_hash(
                "JAILS-PACKAGED-INPUT-1",
                &content,
            ));
        }
    }
    jails_support::codec::hex(&jails_support::codec::domain_hash(
        "JAILS-PACKAGED-INPUTS-1",
        &bytes,
    ))
}

fn project_input_files(project: &Project) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![project.root().join("src/main")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

fn project_root_id(project: &Project) -> String {
    let root = project
        .root()
        .canonicalize()
        .unwrap_or_else(|_| project.root().to_path_buf());
    jails_support::codec::hex(&jails_support::codec::domain_hash(
        "JAILS-PACKAGED-ROOT-1",
        root.to_string_lossy().as_bytes(),
    ))
}

fn file_digest(path: &std::path::Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read packaged artifact {}: {error}\n       fix: rebuild the packaged artifact and retry",
            path.display()
        )
    })?;
    Ok(jails_support::codec::hex(
        &jails_support::codec::domain_hash("JAILS-PACKAGED-ARTIFACT-1", &bytes),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_release_parser_handles_legacy_and_modern_version_output() {
        assert_eq!(
            java_major("openjdk version \"26.0.1\" 2026-04-21"),
            Some(26)
        );
        assert_eq!(java_major("java version \"1.8.0_402\""), Some(8));
        assert_eq!(java_major("openjdk 21.0.8 2025-07-15"), Some(21));
        assert_eq!(java_major("not a java version"), None);
    }

    #[test]
    fn packaged_artifact_proof_is_bound_to_source_bytes() {
        let scratch = jails_support::scratch::ScratchDir::in_temp("packaged-proof").unwrap();
        let root = scratch.path();
        let source = root.join("src/main/java/com/example/App.java");
        let jar = root.join("target/app.jar");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(jar.parent().unwrap()).unwrap();
        fs::write(root.join("pom.xml"), "<project/>\n").unwrap();
        fs::write(&source, "class App {}\n").unwrap();
        fs::write(&jar, "packaged bytes").unwrap();
        let project = Project::inspect(root).unwrap();

        write_artifact_proof(&project, &jar).unwrap();
        verify_artifact_proof(&project, &jar).unwrap();
        fs::write(&source, "class App { int changed; }\n").unwrap();
        assert!(verify_artifact_proof(&project, &jar).is_err());
    }

    #[test]
    fn only_the_single_runtime_jar_is_an_executable_candidate() {
        assert!(is_executable_jar_candidate(std::path::Path::new("app.jar")));
        assert!(!is_executable_jar_candidate(std::path::Path::new(
            "app-plain.jar"
        )));
        assert!(!is_executable_jar_candidate(std::path::Path::new(
            "app-sources.jar"
        )));
        assert!(!is_executable_jar_candidate(std::path::Path::new(
            "app.jar.original"
        )));
    }
}
