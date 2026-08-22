//! The checks that ask the *machine*, not the project.
//!
//! Is there a Maven? Which JDK will actually run? Is the container engine up,
//! and is it the one Testcontainers will find? Is the port free?
//!
//! `abstract.md` §6.2 predicted this seam: as `doctor` derives more from
//! `add::plan_for`, what survives as hand-written is "the checks that probe the
//! environment" -- because no plan can carry the fact that `docker` here is
//! podman's shim, or that Testcontainers reads a different socket from the one
//! `jails start` succeeded against. Those live here, and they are the ones that
//! stay.
//!
//! Read-only, like the rest of `doctor`: nothing here starts, stops or writes
//! anything, so it is safe to run mid-debug.

use super::{Check, Status};
use crate::pom;
use crate::run;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::wiring::property_value;

pub(super) fn maven_check(root: &Path) -> Check {
    let binary = run::maven_binary(root);
    let label = binary.display().to_string();
    if binary.is_absolute() || label.starts_with("./") {
        return Check::new(Status::Ok, "maven", format!("project wrapper ({label})"));
    }
    if run::find_on_path(&label) {
        return Check::new(Status::Ok, "maven", format!("{label} on PATH (no wrapper)"));
    }
    Check::new(
        Status::Fail,
        "maven",
        "no ./mvnw in the project and no mvn/mvnd on PATH",
    )
    .fix("install Maven, or run `mvn -N wrapper:wrapper` once in this project")
}

/// The mismatch that produces `invalid target release` or a compile that
/// simply never happens: a JDK older than what the pom asks javac for.
pub(super) fn jdk_check(pom_text: &str) -> Check {
    let target = pom::release_level(pom_text);
    let installed = java_major();
    match (target, installed) {
        (None, _) => Check::new(
            Status::Warn,
            "jdk",
            "pom.xml sets no Java release level; Maven will default to something ancient",
        )
        .fix(format!(
            "add <maven.compiler.release>{}</maven.compiler.release> to <properties>",
            pom::TARGET_RELEASE
        )),
        (Some(want), None) => Check::new(
            Status::Skip,
            "jdk",
            format!("project targets Java {want}; no `java` on PATH to compare against"),
        ),
        (Some(want), Some(have)) if have < want => Check::new(
            Status::Fail,
            "jdk",
            format!("project targets Java {want}, but `java` on PATH is {have}"),
        )
        .fix(format!(
            "use a JDK {want}+ (`mise exec java@{want} -- jails ...`, or set JAVA_HOME)"
        )),
        (Some(want), Some(have)) => Check::new(
            Status::Ok,
            "jdk",
            format!("java {have} on PATH, project targets {want}"),
        ),
    }
}

pub(super) fn java_major() -> Option<u32> {
    let java = std::env::var_os("JAVA_HOME")
        .map(|home| Path::new(&home).join("bin/java"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| Path::new("java").to_path_buf());
    let out = Command::new(java).arg("-version").output().ok()?;
    // `java -version` writes to stderr, not stdout -- has done since 1.0.
    let text = String::from_utf8_lossy(&out.stderr);
    parse_java_major(&text)
}

/// `openjdk version "26.0.1" 2026-...` -> 26. Also handles the 1.8 spelling,
/// which reports as 8 so that a numeric comparison against a release level
/// works.
pub(super) fn parse_java_major(text: &str) -> Option<u32> {
    let at = text.find("version \"")? + "version \"".len();
    let rest = &text[at..];
    let value = &rest[..rest.find('"')?];
    let mut parts = value.split(['.', '-', '_']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        return parts.next()?.parse().ok();
    }
    Some(first)
}

/// Whether the compose provider is one `spring-boot-docker-compose` can drive.
///
/// This is a static fact about the machine that decides whether the
/// application can start at all, and until now it was only discoverable by
/// running it and reading `jails why` afterwards.
///
/// `spring-boot-docker-compose` shells out with Docker Compose v2 syntax
/// (`--ansi never`, `config --format=json`). podman-compose spells the first
/// `--no-ansi` and has no `--format` at all, so it exits 2 and the app dies
/// during startup, before any of its own code runs.
///
/// The distinguishing string is the provider's own version banner: real
/// Compose v2 says "Docker Compose version v…" whatever is underneath it,
/// including when it is a CLI plugin driving podman over `DOCKER_HOST` --
/// which is the configuration that works and the one this recommends.
///
/// Note the fix jails' `why` rule used to suggest,
/// `spring.docker.compose.enabled=false`, trades one failure for another: it
/// also removes the datasource URL the module was contributing, so the app
/// then dies on "no database URL" instead. Installing real Compose v2 is the
/// fix that leaves nothing broken.
pub(super) fn compose_provider_check(pom_text: &str) -> Option<Check> {
    if !pom::has_dependency(
        pom_text,
        "org.springframework.boot",
        "spring-boot-docker-compose",
    ) {
        return None;
    }
    // Through the same resolver `compose.rs` runs with, so this reports the
    // provider jails would actually drive. Hardcoding `docker` here meant a
    // machine with only the standalone `docker-compose` had `jails start`
    // working while this said Docker was missing.
    let spec =
        crate::process::compose_spec(["version"])?.output(crate::process::OutputMode::Capture);
    let done = crate::process::run(&spec, crate::process::Diagnostics::Normal).ok()?;
    Some(classify_compose_provider(&done.stdout_string()))
}

/// Read a `docker compose version` banner. Split out from the subprocess so
/// the interesting half can be tested.
///
/// The banner is noisier than it looks: podman's docker shim prints its own
/// notices around the real output, so this matches a substring rather than
/// parsing a line.
pub(super) fn classify_compose_provider(banner: &str) -> Check {
    if banner.contains("Docker Compose version") {
        let version = banner
            .lines()
            .find(|l| l.contains("Docker Compose version"))
            .unwrap_or("Docker Compose v2")
            .trim();
        return Check::new(
            Status::Ok,
            "compose provider",
            format!("{version} -- spring-boot-docker-compose can drive it"),
        );
    }

    let provider = if banner.contains("podman-compose") {
        "podman-compose"
    } else {
        "an unrecognised compose provider"
    };
    Check::new(
        Status::Fail,
        "compose provider",
        format!(
            "spring-boot-docker-compose is on the classpath but `docker compose` is \
             {provider} -- it rejects the Compose v2 syntax the module uses \
             (--ansi never, config --format=json) and the application dies during startup"
        ),
    )
    .fix(
        "install Compose v2 as a docker CLI plugin (~/.docker/cli-plugins/docker-compose); \
         it drives podman fine over DOCKER_HOST",
    )
}

/// Bare `docker info`, no `--format`. On a machine where `docker` is
/// podman's Docker-CLI emulation, `--format '{{.ServerVersion}}'` fails
/// against podman's differently-shaped info report and exits 125 -- which
/// would report a perfectly healthy engine as dead.
pub(super) fn docker_daemon_running() -> bool {
    let Some(docker) = crate::process::docker_program() else {
        return false;
    };
    Command::new(docker)
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Names of running containers. Deliberately `docker ps`, not `docker
/// compose ps --services`: the compose subcommand is provided by whichever
/// external compose implementation is installed, and podman-compose (the
/// provider behind podman's `docker` shim) does not accept `--services
/// --status`. `docker ps --format` works identically on both.
pub(super) fn running_containers() -> Vec<String> {
    let Some(docker) = crate::process::docker_program() else {
        // Only the standalone `docker-compose` is installed, so there is no
        // Docker CLI to ask. Reporting "nothing running" would be a guess
        // dressed as a fact; an empty list is what callers treat as unknown.
        return Vec::new();
    };
    let out = Command::new(docker)
        .args(["ps", "--format", "{{.Names}}"])
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Whether a compose service is up. Compose derives a container name by
/// joining the project name, the service name and an index (`rewards_
/// postgres_1` under podman-compose, `rewards-postgres-1` under Docker
/// Compose v2), so the service name is matched as a delimited segment
/// rather than compared whole.
pub(super) fn service_is_running(service: &str, containers: &[String]) -> bool {
    containers
        .iter()
        .any(|name| name.split(['_', '-']).any(|segment| segment == service))
}

/// A bounded TCP connect. `doctor` must never hang: a firewalled host would
/// otherwise stall the whole report on the default connect timeout.
pub(super) fn tcp_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addrs: Vec<SocketAddr> = addrs.collect();
    addrs
        .iter()
        .any(|addr| TcpStream::connect_timeout(addr, timeout).is_ok())
}

pub(super) fn port_checks(root: &Path) -> Vec<Check> {
    let properties = root.join("src/main/resources/application.properties");
    let text = std::fs::read_to_string(&properties).unwrap_or_default();
    let mut ports = vec![("http port", "server.port", 8080_u16)];
    if let Some(port) =
        property_value(&text, "management.server.port").and_then(|value| value.parse::<u16>().ok())
    {
        ports.push(("management port bind", "management.server.port", port));
    }
    ports
        .into_iter()
        .map(|(label, property, default)| {
            let configured = property_value(&text, property)
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(default);
            if tcp_reachable("localhost", configured, Duration::from_millis(250)) {
                Check::new(
                    Status::Warn,
                    label,
                    format!(
                        "something is already listening on {configured} -- the application will fail to bind"
                    ),
                )
                .fix(format!(
                    "stop it, or set {property} to a free port (`lsof -i :{configured}`)"
                ))
            } else {
                Check::new(Status::Ok, label, format!("{configured} is free"))
            }
        })
        .collect()
}

/// The rootless podman socket, if the user socket is where systemd puts it.
pub(super) fn podman_socket() -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = Path::new(&runtime).join("podman/podman.sock");
    socket.exists().then_some(socket)
}

/// The two places Testcontainers looks, in its own order: the environment
/// variable wins, then the file in the user's home directory. **Not** the
/// classpath -- `TestcontainersConfiguration` reads
/// `~/.testcontainers.properties` for this setting and a project-local file
/// is never consulted.
pub(super) fn reuse_enabled() -> bool {
    if let Some(value) = std::env::var_os("TESTCONTAINERS_REUSE_ENABLE") {
        return value.to_string_lossy().trim() == "true";
    }
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(Path::new(&home).join(".testcontainers.properties"))
    else {
        return false;
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .any(|line| line.replace(' ', "") == "testcontainers.reuse.enable=true")
}

/// How many containers carry Testcontainers' reuse hash label.
///
/// `docker ps -a --format` rather than `--filter ... --quiet`, for the same
/// reason the compose checks avoid Docker-specific spellings: this machine's
/// `docker` is podman's shim, and both understand this form.
pub(super) fn reusable_containers() -> usize {
    let Some(docker) = crate::process::docker_program() else {
        return 0;
    };
    let spec = crate::process::CommandSpec::new(docker)
        .args([
            "ps",
            "-a",
            "--filter",
            "label=org.testcontainers.hash",
            "--format",
            "{{.Names}}",
        ])
        .output(crate::process::OutputMode::Capture);
    match crate::process::run(&spec, crate::process::Diagnostics::Normal) {
        Ok(done) if done.status.success() => done
            .stdout_string()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        _ => 0,
    }
}

/// `jails start` takes the capability name (`db`), not the compose service
/// name (`postgres`), so a fix line has to translate back.
pub(super) fn runtime_flag(service: &str) -> &str {
    match service {
        "postgres" => "db",
        other => other,
    }
}

/// Top-level service names in a compose file. Two-space indentation under a
/// `services:` key is the compose spec's own shape and the only one jails
/// writes; a hand-edited file with different indentation just reports fewer
/// services, which is the safe direction.
pub(super) fn declared_services(yaml: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_services = false;
    for line in yaml.lines() {
        if line.starts_with("services:") {
            in_services = true;
            continue;
        }
        if in_services && !line.starts_with(' ') && !line.trim().is_empty() {
            in_services = false;
        }
        if !in_services {
            continue;
        }
        let trimmed = line.trim_end();
        let indent = trimmed.len() - trimmed.trim_start().len();
        if indent == 2
            && let Some(name) = trimmed.trim().strip_suffix(':')
            && !name.starts_with('#')
        {
            found.push(name.to_string());
        }
    }
    found
}
