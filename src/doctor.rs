//! `jails doctor` -- everything that has to be true before the application
//! can start, checked in one pass and reported as a list.
//!
//! The command exists for a specific failure shape: the app does not come
//! up, the stack trace names a Spring internal, and the actual cause is
//! three layers away -- Docker is not running, the JDK on PATH is older than
//! the release the pom targets, a `@Repository` lost its annotation, port
//! 8080 is still held by yesterday's run. Each of those is cheap to test
//! directly and expensive to infer from a trace.
//!
//! Two rules keep it honest. Nothing here writes, starts, or stops anything
//! -- `doctor` is safe to run at any moment, including mid-debug. And every
//! failing check carries the command that fixes it, because a diagnosis the
//! reader has to translate into an action has only moved the work.

use std::fmt::Write as _;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::Result;
use crate::compose;
use crate::generate::find_project_root;
use crate::inspect;
use crate::pom;
use crate::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Checked, and fine.
    Ok,
    /// Checked, and broken in a way that will stop the app from working.
    Fail,
    /// Worth knowing, but not on its own a reason the app will not start.
    Warn,
    /// Could not be checked from here (a tool is missing, or the check would
    /// need the app running). Never counted as a failure.
    Skip,
}

impl Status {
    fn mark(self) -> &'static str {
        match self {
            Status::Ok => "ok  ",
            Status::Fail => "FAIL",
            Status::Warn => "warn",
            Status::Skip => "--  ",
        }
    }
}

struct Check {
    status: Status,
    title: String,
    detail: String,
    /// The command that fixes it. Empty when there is nothing to run.
    fix: String,
}

impl Check {
    fn new(status: Status, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status,
            title: title.into(),
            detail: detail.into(),
            fix: String::new(),
        }
    }

    fn fix(mut self, command: impl Into<String>) -> Self {
        self.fix = command.into();
        self
    }
}

pub fn doctor() -> Result<()> {
    let root = find_project_root()?;
    let checks = run_checks(&root);

    let title_width = checks.iter().map(|c| c.title.len()).max().unwrap_or(0);
    for check in &checks {
        println!(
            "{}  {:title_width$}  {}",
            check.status.mark(),
            check.title,
            check.detail
        );
        if !check.fix.is_empty() {
            println!("{:width$}      fix: {}", "", check.fix, width = title_width);
        }
    }

    let failures = checks.iter().filter(|c| c.status == Status::Fail).count();
    let warnings = checks.iter().filter(|c| c.status == Status::Warn).count();
    println!();
    if failures == 0 && warnings == 0 {
        println!("{} checks, all clear.", checks.len());
        return Ok(());
    }
    println!(
        "{} checks: {failures} failing, {warnings} warning(s).",
        checks.len()
    );
    if failures > 0 {
        // A non-zero exit is what makes `jails doctor && jails run` a usable
        // habit, so the failure is deliberately quiet -- the list above has
        // already said everything, and main.rs would otherwise print a
        // second, redundant `jails: ...` line.
        return Err(String::new());
    }
    Ok(())
}

fn run_checks(root: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let pom_text = pom::read(root).unwrap_or_default();

    checks.push(project_check(root, &pom_text));
    checks.push(maven_check(root));
    checks.push(jdk_check(&pom_text));
    checks.extend(compose_checks(root, &pom_text));
    checks.extend(database_checks(root, &pom_text));
    checks.push(testcontainers_check(&pom_text));
    checks.push(kafka_check(root, &pom_text));
    checks.push(jackson_check(&pom_text));
    checks.push(port_check(root));
    checks.push(beans_check(root));
    checks
}

fn project_check(root: &Path, pom_text: &str) -> Check {
    if pom_text.is_empty() {
        return Check::new(Status::Fail, "project", "pom.xml is missing or unreadable")
            .fix("jails new <name>");
    }
    let flavor = match pom::flavor(pom_text) {
        pom::Flavor::SpringBoot => "Spring Boot",
        pom::Flavor::PlainMaven => "plain Maven",
    };
    let sources = root.join("src/main/java");
    if !sources.is_dir() {
        return Check::new(
            Status::Fail,
            "project",
            format!("{flavor}, but src/main/java does not exist"),
        );
    }
    Check::new(
        Status::Ok,
        "project",
        format!("{flavor}, root {}", root.display()),
    )
}

fn maven_check(root: &Path) -> Check {
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
fn jdk_check(pom_text: &str) -> Check {
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

fn compose_checks(root: &Path, _pom_text: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    if !compose::exists(root) {
        checks.push(Check::new(
            Status::Skip,
            "compose",
            "no compose.yaml -- this project declares no local services",
        ));
        return checks;
    }
    let yaml = compose::read(root).unwrap_or_default();
    let services = declared_services(&yaml);
    checks.push(Check::new(
        Status::Ok,
        "compose",
        format!("compose.yaml declares: {}", services.join(", ")),
    ));

    if !run::find_on_path("docker") {
        checks.push(
            Check::new(
                Status::Fail,
                "docker",
                "compose.yaml declares services but docker is not on PATH",
            )
            .fix("install Docker, or remove the services with `jails remove db kafka`"),
        );
        return checks;
    }
    if !docker_daemon_running() {
        checks.push(
            Check::new(
                Status::Fail,
                "docker",
                "docker is installed but the daemon is not responding",
            )
            .fix("start Docker (`systemctl --user start docker` / open Docker Desktop)"),
        );
        return checks;
    }

    let running = running_containers();
    for service in &services {
        let up = service_is_running(service, &running);
        checks.push(if up {
            Check::new(Status::Ok, format!("service {service}"), "running")
        } else {
            Check::new(
                Status::Fail,
                format!("service {service}"),
                "declared in compose.yaml but not running",
            )
            .fix(format!("jails start {}", runtime_flag(service)))
        });
    }
    checks
}

fn database_checks(root: &Path, pom_text: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    let yaml = compose::read(root).unwrap_or_default();
    let Some(conn) = compose::postgres_connect(&yaml) else {
        if pom::has_dependency(pom_text, "org.postgresql", "postgresql") {
            checks.push(
                Check::new(
                    Status::Fail,
                    "postgres",
                    "the PostgreSQL driver is a dependency but compose.yaml has no postgres service",
                )
                .fix("jails add db"),
            );
        }
        return checks;
    };

    // A TCP connect is the honest test: the container can be "running" while
    // postgres inside it is still replaying WAL and refusing connections,
    // which is exactly the window a `jails run` right after `jails start`
    // lands in.
    let reachable = tcp_reachable(&conn.host, conn.port, Duration::from_millis(750));
    checks.push(if reachable {
        Check::new(
            Status::Ok,
            "postgres",
            format!(
                "accepting connections on {}:{} (db {}, user {})",
                conn.host, conn.port, conn.database, conn.user
            ),
        )
    } else {
        Check::new(
            Status::Fail,
            "postgres",
            format!("nothing accepting connections on {}:{}", conn.host, conn.port),
        )
        .fix("jails start db")
    });

    let migrations = root.join("src/main/resources/db/migration");
    let count = std::fs::read_dir(&migrations)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0);
    let has_flyway = pom::has_dependency(pom_text, "org.flywaydb", "flyway-core");
    checks.push(match (has_flyway, count) {
        (true, 0) => Check::new(
            Status::Warn,
            "migrations",
            "Flyway is on the classpath but db/migration holds no .sql files -- the schema will be empty",
        )
        .fix("jails g migration create_<table>"),
        (true, n) => Check::new(
            Status::Ok,
            "migrations",
            format!("{n} migration(s) in src/main/resources/db/migration"),
        ),
        (false, n) if n > 0 => Check::new(
            Status::Warn,
            "migrations",
            format!("{n} .sql file(s) in db/migration but Flyway is not a dependency -- nothing will run them"),
        )
        .fix("jails add db"),
        (false, _) => Check::new(Status::Skip, "migrations", "no Flyway, no migration directory"),
    });

    // The two pieces of test-side wiring `add db` installs on Spring. Both
    // are invisible until a @SpringBootTest fails with "Failed to determine
    // a suitable driver class", and both are easy to lose to a rebase.
    if matches!(pom::flavor(pom_text), pom::Flavor::SpringBoot) {
        let factories = root.join("src/test/resources/META-INF/spring.factories");
        let initializer_registered = std::fs::read_to_string(&factories)
            .map(|t| t.contains("ApplicationContextInitializer"))
            .unwrap_or(false);
        checks.push(if initializer_registered {
            Check::new(
                Status::Ok,
                "test datasource",
                "Testcontainers initializer registered in test spring.factories",
            )
        } else {
            Check::new(
                Status::Fail,
                "test datasource",
                "Spring + postgres, but no test-classpath ApplicationContextInitializer -- \
                 @SpringBootTest will fail with \"Failed to determine a suitable driver class\"",
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)")
        });

        let properties = root.join("src/main/resources/application.properties");
        let translation_disabled = std::fs::read_to_string(&properties)
            .map(|t| t.contains("spring.persistence.exceptiontranslation.enabled=false"))
            .unwrap_or(false);
        if !translation_disabled {
            checks.push(
                Check::new(
                    Status::Warn,
                    "exception translation",
                    "spring.persistence.exceptiontranslation.enabled is not disabled -- JDBC \
                     auto-config will CGLIB-proxy every @Repository, which fails on final classes",
                )
                .fix("add spring.persistence.exceptiontranslation.enabled=false to application.properties"),
            );
        }
    }
    checks
}

/// Testcontainers finds its engine through DOCKER_HOST or the well-known
/// `/var/run/docker.sock`, and does *not* read the podman socket that
/// podman's `docker` CLI shim talks to. On a rootless-podman machine the
/// CLI therefore works perfectly while every @SpringBootTest dies with
/// "Could not find a valid Docker environment" -- the two look at different
/// sockets, which is why `jails start` succeeding proves nothing about
/// whether the test suite can start a container.
fn testcontainers_check(pom_text: &str) -> Check {
    // Matched on the groupId alone, not on artifact ids: Testcontainers 2.0
    // renamed every module (`postgresql` -> `testcontainers-postgresql`),
    // and a check that silently stops applying after a dependency bump is
    // worse than no check.
    let uses = pom_text.contains("org.testcontainers");
    if !uses {
        return Check::new(Status::Skip, "testcontainers", "not in use");
    }
    if let Some(host) = std::env::var_os("DOCKER_HOST") {
        return Check::new(
            Status::Ok,
            "testcontainers",
            format!("DOCKER_HOST={}", host.to_string_lossy()),
        );
    }
    if Path::new("/var/run/docker.sock").exists() {
        return Check::new(Status::Ok, "testcontainers", "/var/run/docker.sock is present");
    }
    if let Some(socket) = podman_socket() {
        return Check::new(
            Status::Fail,
            "testcontainers",
            "no DOCKER_HOST and no /var/run/docker.sock, but a rootless podman socket exists -- \
             tests will fail with \"Could not find a valid Docker environment\"",
        )
        .fix(format!(
            "export DOCKER_HOST=unix://{} (and TESTCONTAINERS_RYUK_DISABLED=true for rootless podman)",
            socket.display()
        ));
    }
    Check::new(
        Status::Fail,
        "testcontainers",
        "no DOCKER_HOST, no /var/run/docker.sock, no podman socket -- no container engine for tests to use",
    )
    .fix("start Docker, or `systemctl --user start podman.socket` and export DOCKER_HOST")
}

/// The rootless podman socket, if the user socket is where systemd puts it.
fn podman_socket() -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = Path::new(&runtime).join("podman/podman.sock");
    socket.exists().then_some(socket)
}

fn kafka_check(root: &Path, pom_text: &str) -> Check {
    let has_client = pom::has_dependency(pom_text, "org.apache.kafka", "kafka-clients")
        || pom::has_dependency(pom_text, "org.springframework.boot", "spring-boot-starter-kafka");
    let yaml = compose::read(root).unwrap_or_default();
    let has_broker = yaml.contains("# jails:kafka") || yaml.contains("\n  kafka:");
    match (has_client, has_broker) {
        (false, false) => Check::new(Status::Skip, "kafka", "not in use"),
        (true, true) => Check::new(Status::Ok, "kafka", "client dependency and broker service both present"),
        (true, false) => Check::new(
            Status::Fail,
            "kafka",
            "a Kafka client is on the classpath but compose.yaml declares no broker",
        )
        .fix("jails add kafka"),
        (false, true) => Check::new(
            Status::Warn,
            "kafka",
            "compose.yaml runs a broker but no Kafka client is a dependency",
        )
        .fix("jails add kafka, or `jails remove kafka` to drop the broker"),
    }
}

/// Both Jackson artifacts have to be present and pinned together, or every
/// `LocalDate` silently serialises as `{"year":...}` instead of an ISO
/// string. Spring pulls the second one in transitively, so this only ever
/// bites the plain-Maven flavor.
fn jackson_check(pom_text: &str) -> Check {
    let databind = pom::has_dependency(pom_text, "com.fasterxml.jackson.core", "jackson-databind");
    let jsr310 = pom::has_dependency(
        pom_text,
        "com.fasterxml.jackson.datatype",
        "jackson-datatype-jsr310",
    );
    match (databind, jsr310) {
        (false, _) => Check::new(Status::Skip, "json", "Jackson is not in use"),
        (true, true) => Check::new(Status::Ok, "json", "jackson-databind and jackson-datatype-jsr310 both present"),
        (true, false) => Check::new(
            Status::Fail,
            "json",
            "jackson-databind without jackson-datatype-jsr310 -- java.time values will serialise \
             as objects, not ISO strings",
        )
        .fix("jails add json"),
    }
}

/// "Port 8080 was already in use" is a top-three Spring startup failure and
/// costs nothing to detect before the JVM even starts.
fn port_check(root: &Path) -> Check {
    let properties = root.join("src/main/resources/application.properties");
    let configured = std::fs::read_to_string(&properties)
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.trim().strip_prefix("server.port=")?.trim().parse::<u16>().ok())
        })
        .unwrap_or(8080);
    if tcp_reachable("localhost", configured, Duration::from_millis(250)) {
        Check::new(
            Status::Warn,
            "http port",
            format!("something is already listening on {configured} -- a second app will fail to bind"),
        )
        .fix(format!("stop it, or set server.port to a free port (`lsof -i :{configured}`)"))
    } else {
        Check::new(Status::Ok, "http port", format!("{configured} is free"))
    }
}

/// The static half of a "required a bean of type ... that could not be
/// found" failure, available without starting the context.
fn beans_check(root: &Path) -> Check {
    let (beans, project_types) = inspect::collect_beans(root);
    if beans.is_empty() {
        return Check::new(Status::Skip, "beans", "no Spring stereotypes in src/main/java");
    }
    let supplied = inspect::providers(&beans);
    let mut missing = Vec::new();
    let mut ambiguous = Vec::new();
    for bean in &beans {
        for need in &bean.needs {
            match supplied.get(need.as_str()).map(Vec::len).unwrap_or(0) {
                1 => {}
                // Spring will not choose between candidates, so two is as
                // broken as zero -- and it is the failure a project hits the
                // day it keeps an in-memory fake alongside a real adapter.
                n if n > 1 => ambiguous.push(format!(
                    "{need} has {n} candidates ({})",
                    supplied[need.as_str()].join(", ")
                )),
                _ if project_types.contains(need.as_str()) => {
                    missing.push(format!("{} needs {need}", bean.type_name))
                }
                _ => {}
            }
        }
    }
    if missing.is_empty() && ambiguous.is_empty() {
        return Check::new(
            Status::Ok,
            "beans",
            format!("{} bean(s), every project-typed dependency resolvable", beans.len()),
        );
    }
    let mut detail = String::new();
    if !missing.is_empty() {
        let _ = write!(detail, "unresolvable: {}", missing.join("; "));
    }
    if !ambiguous.is_empty() {
        if !detail.is_empty() {
            detail.push_str("; ");
        }
        ambiguous.dedup();
        let _ = write!(detail, "ambiguous: {}", ambiguous.join("; "));
    }
    Check::new(Status::Fail, "beans", detail).fix(if missing.is_empty() {
        "mark one candidate @Primary, or drop the stereotype from the fake"
    } else {
        "annotate the implementation (@Service/@Repository/@Component) or add an @Bean method"
    })
}

/// `jails start` takes the capability name (`db`), not the compose service
/// name (`postgres`), so a fix line has to translate back.
fn runtime_flag(service: &str) -> &str {
    match service {
        "postgres" => "db",
        other => other,
    }
}

/// Top-level service names in a compose file. Two-space indentation under a
/// `services:` key is the compose spec's own shape and the only one jails
/// writes; a hand-edited file with different indentation just reports fewer
/// services, which is the safe direction.
fn declared_services(yaml: &str) -> Vec<String> {
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

fn java_major() -> Option<u32> {
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
fn parse_java_major(text: &str) -> Option<u32> {
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

/// Bare `docker info`, no `--format`. On a machine where `docker` is
/// podman's Docker-CLI emulation, `--format '{{.ServerVersion}}'` fails
/// against podman's differently-shaped info report and exits 125 -- which
/// would report a perfectly healthy engine as dead.
fn docker_daemon_running() -> bool {
    Command::new("docker")
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
fn running_containers() -> Vec<String> {
    let out = Command::new("docker")
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
fn service_is_running(service: &str, containers: &[String]) -> bool {
    containers.iter().any(|name| {
        name.split(['_', '-']).any(|segment| segment == service)
    })
}

/// A bounded TCP connect. `doctor` must never hang: a firewalled host would
/// otherwise stall the whole report on the default connect timeout.
fn tcp_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addrs: Vec<SocketAddr> = addrs.collect();
    addrs
        .iter()
        .any(|addr| TcpStream::connect_timeout(addr, timeout).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_version_output_yields_a_major_number() {
        let modern = "openjdk version \"26.0.1\" 2026-01-20\nOpenJDK Runtime Environment";
        assert_eq!(parse_java_major(modern), Some(26));
        let legacy = "java version \"1.8.0_401\"";
        assert_eq!(parse_java_major(legacy), Some(8));
        let ea = "openjdk version \"27-ea\" 2026-09-15";
        assert_eq!(parse_java_major(ea), Some(27));
        assert_eq!(parse_java_major("no version here"), None);
    }

    #[test]
    fn declared_services_reads_top_level_names_only() {
        let yaml = "\
services:
  postgres:
    image: postgres:17-alpine
    ports:
      - \"5432:5432\"
  kafka:
    image: apache/kafka:4.1.0
volumes:
  postgres-data:
";
        assert_eq!(declared_services(yaml), vec!["postgres", "kafka"]);
    }

    #[test]
    fn declared_services_skips_marker_comments() {
        let yaml = "services:\n  # jails:db\n  postgres:\n    image: x\n  # /jails:db\n";
        assert_eq!(declared_services(yaml), vec!["postgres"]);
    }

    #[test]
    fn a_jdk_older_than_the_target_release_fails() {
        // release_level reads the pom; the JDK half is checked separately
        // because it depends on the machine.
        let old = "<maven.compiler.release>27</maven.compiler.release>";
        assert_eq!(pom::release_level(old), Some(27));
    }

    #[test]
    fn jackson_databind_without_jsr310_is_a_failure() {
        let pom = "<dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        assert_eq!(jackson_check(pom).status, Status::Fail);
    }

    #[test]
    fn both_jackson_artifacts_pass() {
        let pom = "<dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>\
                   <dependency><groupId>com.fasterxml.jackson.datatype</groupId>\
                   <artifactId>jackson-datatype-jsr310</artifactId></dependency>";
        assert_eq!(jackson_check(pom).status, Status::Ok);
    }

    #[test]
    fn testcontainers_is_detected_under_both_module_naming_schemes() {
        let v1 = "<groupId>org.testcontainers</groupId><artifactId>postgresql</artifactId>";
        let v2 = "<groupId>org.testcontainers</groupId><artifactId>testcontainers-postgresql</artifactId>";
        assert_ne!(testcontainers_check(v1).status, Status::Skip);
        assert_ne!(testcontainers_check(v2).status, Status::Skip);
        assert_eq!(testcontainers_check("<project/>").status, Status::Skip);
    }

    #[test]
    fn a_service_is_matched_inside_the_compose_container_name() {
        let containers = vec!["rewards_postgres_1".to_string(), "other-kafka-1".to_string()];
        assert!(service_is_running("postgres", &containers));
        assert!(service_is_running("kafka", &containers));
        assert!(!service_is_running("redis", &containers));
        // A service name that is only a substring of a segment must not match.
        assert!(!service_is_running("post", &containers));
    }

    #[test]
    fn runtime_flag_translates_the_compose_service_name() {
        assert_eq!(runtime_flag("postgres"), "db");
        assert_eq!(runtime_flag("kafka"), "kafka");
    }
}
