//! Developer-tool probes for `jails doctor`.
//!
//! The report layer owns project inspection and rendering. This command layer
//! owns executable lookup and version processes, matching the tools the
//! application gateways actually launch without installing or starting them.

use crate::model::Project;
use jails_report::doctor::{Check, Status};
use jails_support::Result;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub use jails_report::doctor::setup;

const CURL_INSTALL: &str = "https://curl.se/download.html";
const PGCLI_INSTALL: &str = "https://www.pgcli.com/install";
const POSTGRES_INSTALL: &str = "https://www.postgresql.org/download/";
const COMPOSE_INSTALL: &str = "https://docs.docker.com/compose/install/";
const JDK_INSTALL: &str = "https://adoptium.net/installation/";
const JSHELL_GUIDE: &str = "https://docs.oracle.com/en/java/javase/26/jshell/index.html";
const MAVEN_INSTALL: &str = "https://maven.apache.org/install.html";
const GRADLE_INSTALL: &str = "https://docs.gradle.org/current/userguide/installation.html";

/// `additional` carries the checks this crate cannot ask.
///
/// A canonical project's managed-output questions are answered from the lock,
/// which lives above `jails-report` and beside the binary; passing them in
/// keeps `jails-workspace` out of the read-only crate rather than pulling the
/// compiler ladder into it.
pub fn doctor(json: bool, additional: Vec<Check>) -> Result<()> {
    let root = crate::find_project_root()?;
    let project = Project::inspect(&root)?;
    let mut checks = developer_tool_checks(&project);
    checks.extend(additional);
    jails_report::doctor::doctor(&project, json, checks)
}

fn developer_tool_checks(project: &Project) -> Vec<Check> {
    let mut checks = Vec::new();
    if has_http_routes(project) {
        checks.push(probe(
            project,
            "curl executable",
            Path::new("curl"),
            &["--version"],
            true,
            CURL_INSTALL,
        ));
    }
    if has_postgres(project) {
        checks.extend(postgres_clients(project));
    }
    if has_compose(project) {
        checks.push(compose_probe(project));
    }

    let java = selected_java();
    let resolved_java = resolve_executable(project, &java);
    checks.push(probe(
        project,
        "java executable",
        &java,
        &["-version"],
        true,
        JDK_INSTALL,
    ));
    if is_spring(project) {
        let jshell = resolved_java
            .map(|path| {
                path.with_file_name(if cfg!(windows) {
                    "jshell.exe"
                } else {
                    "jshell"
                })
            })
            .unwrap_or_else(|| PathBuf::from("jshell"));
        checks.push(probe(
            project,
            "jshell executable",
            &jshell,
            &["--version"],
            true,
            JSHELL_GUIDE,
        ));
    }

    match project.build() {
        crate::build::Build::Maven => checks.push(maven_probe(project)),
        crate::build::Build::Gradle => checks.push(gradle_probe(project)),
        crate::build::Build::Bare | crate::build::Build::Foreign(_) => {}
    }
    checks
}

fn probe(
    project: &Project,
    title: &str,
    program: &Path,
    args: &[&str],
    required: bool,
    install: &str,
) -> Check {
    let Some(path) = resolve_executable(project, program) else {
        return unavailable(title, program, required, install);
    };
    let output = Command::new(&path)
        .args(args)
        .current_dir(project.root())
        .output();
    let Ok(output) = output else {
        return unavailable(title, &path, required, install);
    };
    if !output.status.success() {
        return Check::new(
            if required { Status::Fail } else { Status::Warn },
            title,
            format!("{} rejected its version probe", path.display()),
        )
        .fix(format!("repair or reinstall it from {install}"));
    }
    let version = version_line(&output.stdout, &output.stderr)
        .unwrap_or_else(|| "version not reported".to_string());
    Check::new(
        Status::Ok,
        title,
        format!("{} -- {version}", path.display()),
    )
}

fn unavailable(title: &str, program: &Path, required: bool, install: &str) -> Check {
    Check::new(
        if required { Status::Fail } else { Status::Warn },
        title,
        format!(
            "{} was not resolved in the command environment",
            program.display()
        ),
    )
    .fix(format!(
        "install it from {install}, then ensure its executable is on PATH"
    ))
}

/// The PostgreSQL clients, and one failure only when there is no client at all.
///
/// **Requiring `pgcli` made a machine with the ordinary client fail.** It is
/// what `jails db console` defaults to, so it was probed as required; `psql`
/// is what `jails db`, `jails migrate --check` and every live-schema question
/// shell out to, and it was the optional one. `pgcli` is a pip package. Every
/// CI runner and every server has `psql` and not it, so `doctor` called those
/// projects broken over a convenience -- and a test asserting `doctor` exits
/// zero passed on a laptop and failed everywhere else for eighteen hours.
///
/// Each client is still reported with its own path and version, because *which
/// psql* is a question a reader asks. What changed is that a missing one is a
/// warning naming the command that will refuse without it, and the failure is
/// the case doctor is actually asking about: no client at all.
fn postgres_clients(project: &Project) -> Vec<Check> {
    let pgcli = probe(
        project,
        "pgcli executable",
        Path::new("pgcli"),
        &["--version"],
        false,
        PGCLI_INSTALL,
    );
    let psql = probe(
        project,
        "psql executable",
        Path::new("psql"),
        &["--version"],
        false,
        POSTGRES_INSTALL,
    );
    let (has_pgcli, has_psql) = (pgcli.status() == Status::Ok, psql.status() == Status::Ok);
    let mut checks =
        vec![
            match has_pgcli {
                true => pgcli,
                false => pgcli
                    .note("`jails db console` defaults to it -- pass `--client psql` without it"),
            },
            match has_psql {
                true => psql,
                false => psql
                    .note("`jails migrate --check` and the live-schema checks refuse without it"),
            },
        ];
    if !has_pgcli && !has_psql {
        checks.push(
            Check::new(
                Status::Fail,
                "postgres client",
                "no PostgreSQL client was resolved, so no database console or live-schema check can run",
            )
            .fix(format!(
                "install one of them -- {POSTGRES_INSTALL} or {PGCLI_INSTALL} -- then ensure it is on PATH"
            )),
        );
    }
    checks
}

fn compose_probe(project: &Project) -> Check {
    let Some((program, prefix)) = crate::process::compose_program() else {
        return unavailable(
            "compose executable",
            Path::new("docker compose"),
            true,
            COMPOSE_INSTALL,
        );
    };
    let mut args = prefix.to_vec();
    args.push("version");
    probe(
        project,
        "compose executable",
        Path::new(program),
        &args,
        true,
        COMPOSE_INSTALL,
    )
}

fn maven_probe(project: &Project) -> Check {
    let binary = crate::maven::binary(project.root());
    if is_wrapper(&binary, "mvnw") {
        return wrapper_probe(
            project,
            "maven executable",
            &binary,
            ".mvn/wrapper/maven-wrapper.properties",
            "apache-maven-",
            MAVEN_INSTALL,
        );
    }
    probe(
        project,
        "maven executable",
        &binary,
        &["--version"],
        true,
        MAVEN_INSTALL,
    )
}

fn gradle_probe(project: &Project) -> Check {
    let wrapper = project.root().join(if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    });
    if wrapper.is_file() {
        return wrapper_probe(
            project,
            "gradle executable",
            &wrapper,
            "gradle/wrapper/gradle-wrapper.properties",
            "gradle-",
            GRADLE_INSTALL,
        );
    }
    probe(
        project,
        "gradle executable",
        Path::new("gradle"),
        &["--version"],
        true,
        GRADLE_INSTALL,
    )
}

fn wrapper_probe(
    project: &Project,
    title: &str,
    program: &Path,
    properties: &str,
    prefix: &str,
    install: &str,
) -> Check {
    let Some(path) = resolve_executable(project, program) else {
        return unavailable(title, program, true, install);
    };
    let version = fs::read_to_string(project.root().join(properties))
        .ok()
        .and_then(|body| wrapper_version(&body, prefix));
    let Some(version) = version else {
        return Check::new(
            Status::Fail,
            title,
            format!(
                "{} has no readable pinned distribution version",
                path.display()
            ),
        )
        .fix(format!("repair the project wrapper using {install}"));
    };
    Check::new(
        Status::Ok,
        title,
        format!(
            "{} -- {version} (pinned wrapper distribution)",
            path.display()
        ),
    )
}

fn wrapper_version(properties: &str, prefix: &str) -> Option<String> {
    let distribution = properties
        .lines()
        .find_map(|line| line.trim().strip_prefix("distributionUrl="))?;
    let file = distribution.rsplit('/').next()?;
    let version = file.strip_prefix(prefix)?;
    version
        .split_once("-bin.")
        .or_else(|| version.split_once("-all."))
        .map(|(version, _)| version.to_string())
}

fn version_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    [stdout, stderr]
        .into_iter()
        .flat_map(|bytes| {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .map(|line| line.trim().to_string())
        .find(|line| !line.is_empty() && !line.starts_with("Picked up "))
}

fn resolve_executable(project: &Project, program: &Path) -> Option<PathBuf> {
    if program.is_absolute() {
        return executable(program).then(|| canonical(program));
    }
    if program.components().count() > 1 {
        let path = project.root().join(program);
        return executable(&path).then(|| canonical(&path));
    }
    let path = std::env::var_os("PATH")?;
    let names = executable_names(program.as_os_str());
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| executable(path))
        .map(|path| canonical(&path))
}

fn executable_names(name: &OsStr) -> Vec<OsString> {
    let mut names = vec![name.to_os_string()];
    if cfg!(windows) && Path::new(name).extension().is_none() {
        let extensions =
            std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        names.extend(
            extensions
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| {
                    OsString::from(format!("{}{}", name.to_string_lossy(), extension))
                }),
        );
    }
    names
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn selected_java() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .map(|home| {
            home.join(if cfg!(windows) {
                "bin/java.exe"
            } else {
                "bin/java"
            })
        })
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn is_wrapper(path: &Path, stem: &str) -> bool {
    path.file_stem().is_some_and(|name| name == stem)
}

fn has_http_routes(project: &Project) -> bool {
    java_sources(project).any(|body| {
        [
            "@GetMapping",
            "@PostMapping",
            "@PutMapping",
            "@PatchMapping",
            "@DeleteMapping",
            "@RequestMapping",
        ]
        .iter()
        .any(|annotation| body.contains(annotation))
    })
}

fn is_spring(project: &Project) -> bool {
    java_sources(project).any(|body| body.contains("@SpringBootApplication"))
}

fn java_sources(project: &Project) -> impl Iterator<Item = String> {
    crate::java::source_files(&project.root().join("src/main/java"))
        .into_iter()
        .filter_map(|path| fs::read_to_string(path).ok())
}

fn has_postgres(project: &Project) -> bool {
    ["compose.yaml", "compose.yml"]
        .iter()
        .filter_map(|name| fs::read_to_string(project.root().join(name)).ok())
        .any(|body| body.contains("postgres"))
}

fn has_compose(project: &Project) -> bool {
    project.root().join("compose.yaml").is_file() || project.root().join("compose.yml").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_versions_are_read_without_launching_or_downloading() {
        assert_eq!(
            wrapper_version(
                "distributionUrl=https\\://repo/maven/apache-maven-3.9.11-bin.zip",
                "apache-maven-"
            ),
            Some("3.9.11".into())
        );
        assert_eq!(
            wrapper_version(
                "distributionUrl=https\\://services/gradle-9.1.0-all.zip",
                "gradle-"
            ),
            Some("9.1.0".into())
        );
    }
}
