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
    checks.extend(compose_provider_check(&pom_text));
    checks.extend(database_checks(root, &pom_text));
    checks.extend(in_memory_adapter_check(root, &pom_text));
    checks.push(testcontainers_check(&pom_text));
    checks.extend(container_reuse_check(&pom_text));
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
    // Before anything else about the project: can Maven open this pom at
    // all? `pom::read` falls back to an empty string, so without this every
    // check below happily reported on a project no goal can run against --
    // fifteen greens over a build that cannot start (plan.md §8.9).
    if let Some((problem, fix)) = pom::problems(pom_text).into_iter().next() {
        return Check::new(
            Status::Fail,
            "project",
            format!("{flavor}, and Maven cannot read pom.xml: {problem}"),
        )
        .fix(&fix);
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
fn compose_provider_check(pom_text: &str) -> Option<Check> {
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
fn classify_compose_provider(banner: &str) -> Check {
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
            format!(
                "nothing accepting connections on {}:{}",
                conn.host, conn.port
            ),
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
    // Counting the files answers the wrong question. `flyway-core` alone runs
    // nothing on Boot 4 -- the auto-configuration lives in the separate
    // `spring-boot-flyway` module -- and the failure is silent: no error, no
    // warning, no Flyway log line, then `relation "..." does not exist`. So
    // the check is "will these run", not "do these exist".
    let is_spring = matches!(pom::flavor(pom_text), pom::Flavor::SpringBoot);
    let has_boot_flyway =
        pom::has_dependency(pom_text, "org.springframework.boot", "spring-boot-flyway");
    if is_spring && has_flyway && !has_boot_flyway {
        checks.push(
            Check::new(
                Status::Fail,
                "migrations",
                format!(
                    "{count} .sql file(s) and flyway-core, but not spring-boot-flyway -- \
                     Boot 4 keeps Flyway's auto-configuration in that module, so nothing \
                     runs them and nothing says so"
                ),
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
        );
    } else {
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
    }

    // The two pieces of test-side wiring `add db` installs on Spring. Both
    // are invisible until a @SpringBootTest fails with "Failed to determine
    // a suitable driver class", and both are easy to lose to a rebase.
    if matches!(pom::flavor(pom_text), pom::Flavor::SpringBoot) {
        // What matters is that a container bean exists *and* that every
        // @SpringBootTest can see one. Checking the file alone would pass on
        // a project where a rebase dropped the @Import and every context test
        // is red; checking the imports alone would pass on a project whose
        // config file was deleted.
        let (container_config, unimported) = test_container_wiring(root);
        checks.push(match (&container_config, unimported.as_slice()) {
            (None, _) => Check::new(
                Status::Fail,
                "test datasource",
                "Spring + postgres, but no @ServiceConnection container config on the test \
                 classpath -- @SpringBootTest will fail with \"Failed to determine a suitable \
                 driver class\"",
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
            (Some(class), []) => Check::new(
                Status::Ok,
                "test datasource",
                format!("{class} declares an @ServiceConnection container, imported where needed"),
            ),
            (Some(class), missing) => Check::new(
                Status::Fail,
                "test datasource",
                format!(
                    "{} @SpringBootTest class(es) do not import {class} -- they will fail with \
                     \"Failed to determine a suitable driver class\": {}",
                    missing.len(),
                    missing.join(", ")
                ),
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
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
/// The test-side container wiring: which class declares the container, and
/// which `@SpringBootTest` classes cannot see it.
///
/// Deliberately textual, like the rest of jails' Java reading -- it answers on
/// a project that does not compile, which is the case that matters when
/// something is already broken.
/// Does this test import `class`, however the annotation is spelled?
///
/// Not a substring match on `@Import(Foo.class)`: Spring's `@Import` is not
/// repeatable, so jails' own splicer *merges* -- a test that also needs its
/// own containers ends up with
/// `@Import({SomeIT.Containers.class, TestcontainersConfig.class})`, which the
/// literal form misses. A check that goes red on correctly wired code is
/// worse than no check, because the fix it names changes nothing.
fn imports_config(text: &str, class: &str) -> bool {
    crate::java::annotations(text)
        .iter()
        .filter(|a| a.name == "Import")
        .any(|a| {
            a.args
                .split(',')
                .map(|member| {
                    member
                        .trim()
                        .trim_start_matches(['{', '('])
                        .trim_end_matches(['}', ')'])
                        .trim()
                })
                .any(|member| {
                    member == format!("{class}.class")
                        || member.ends_with(&format!(".{class}.class"))
                })
        })
}

fn test_container_wiring(root: &Path) -> (Option<String>, Vec<String>) {
    let mut config: Option<String> = None;
    let mut boot_tests: Vec<(String, String)> = Vec::new();

    let mut stack = vec![root.join("src/test/java")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.extension().is_some_and(|e| e == "java") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            // Read the annotations rather than the bytes. Both facts here
            // have a decoy in the tree: `TestcontainersConfig`'s own Javadoc
            // shows a `@SpringBootTest` usage example inside `{@code ...}`,
            // and `g event` writes a `Containers` @TestConfiguration *nested
            // inside* its messaging IT. A substring scan reads the first as a
            // test and the second as the project's container config, which is
            // how `doctor` came to name the wrong class and then report every
            // other test as missing an import of it.
            let annotations = crate::java::annotations(&text);
            let on_the_top_level_type = |name: &str| {
                annotations.iter().any(|a| {
                    a.name == name && a.target == crate::java::Target::Type(stem.clone())
                })
            };
            if on_the_top_level_type("TestConfiguration")
                && annotations.iter().any(|a| a.name == "ServiceConnection")
            {
                config = Some(stem.clone());
            }
            if on_the_top_level_type("SpringBootTest") {
                boot_tests.push((stem, text));
            }
        }
    }

    let unimported = match &config {
        Some(class) => {
            let mut missing: Vec<String> = boot_tests
                .into_iter()
                // The config class itself may carry @SpringBootTest in a
                // sample snippet; it obviously does not import itself.
                .filter(|(stem, text)| stem != class && !imports_config(text, class))
                .map(|(stem, _)| stem)
                .collect();
            missing.sort();
            missing
        }
        None => Vec::new(),
    };
    (config, unimported)
}

/// A project with a real database whose repository bean is still the
/// in-memory one.
///
/// This is the quiet half of the two-adapter problem. Two annotated
/// adapters is loud -- Spring refuses to start and `jails beans` reports the
/// ambiguity. *One*, on the in-memory adapter, with a `DataSource` sitting
/// right there, starts perfectly and serves every request out of a
/// `ConcurrentHashMap` that is empty on each boot. Nothing fails. The data
/// just is not there, and the first person to notice is whoever asks why
/// last week's records are gone.
///
/// It is the state a project lands in by scaffolding first and running
/// `jails add db` second, because `add db` does not rewrite an
/// already-generated scaffold -- deliberately: those files may have been
/// edited by hand since, and silently regenerating them would cost more than
/// this check does.
fn in_memory_adapter_check(root: &Path, pom_text: &str) -> Option<Check> {
    if !pom::has_dependency(
        pom_text,
        "org.springframework.boot",
        "spring-boot-starter-jdbc",
    ) {
        return None;
    }
    let mut in_memory_beans = Vec::new();
    let mut stack = vec![root.join("src/main/java")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !stem.starts_with("InMemory") || !path.extension().is_some_and(|e| e == "java") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // The annotation on a declaration, not the word in a Javadoc.
            let annotated = ["@Component", "@Repository"].iter().any(|a| {
                text.contains(&format!("{a}\npublic class"))
                    || text.contains(&format!("{a}\nclass"))
            });
            if annotated {
                in_memory_beans.push(stem.to_string());
            }
        }
    }
    if in_memory_beans.is_empty() {
        return None;
    }
    in_memory_beans.sort();
    Some(
        Check::new(
            Status::Fail,
            "repository bean",
            format!(
                "this project has a DataSource, but {} is still a bean -- \
                 the application starts and serves every request from memory, losing \
                 everything on restart, with no error anywhere",
                in_memory_beans.join(", ")
            ),
        )
        .fix(
            "re-generate the adapter so the JDBC one is the bean: \
             jails destroy repo <Name> && jails g repo <Name> <fields...>",
        ),
    )
}

/// Is container reuse switched on for this machine, and is anything piling up
/// because of it?
///
/// Reuse is the largest single saving available to a suite that starts
/// PostgreSQL, and it is **not** what jails generates -- see
/// `TestcontainersConfig`'s Javadoc for why: the reuse key is a hash of the
/// container's configuration, nothing in that configuration identifies the
/// project, so two applications on the same image share one database and
/// Flyway refuses to start against the other one's migration history.
///
/// So this check does not push anyone towards it. What it does is report the
/// machine flag honestly, and count what reuse has left behind -- a reused
/// container is deliberately not registered with Ryuk, so nothing else will
/// ever mention it.
fn container_reuse_check(pom_text: &str) -> Vec<Check> {
    if !pom_text.contains("org.testcontainers") {
        return vec![Check::new(Status::Skip, "container reuse", "not in use")];
    }
    if !reuse_enabled() {
        return vec![Check::new(
            Status::Skip,
            "container reuse",
            "off for this machine; generated container configs do not ask for it",
        )];
    }
    let kept = reusable_containers();
    let detail = match kept {
        0 => "enabled for this machine; nothing kept".to_string(),
        1 => "enabled for this machine; 1 container kept between runs".to_string(),
        n => format!("enabled for this machine; {n} containers kept between runs"),
    };
    // Not a failure at any count: they are *supposed* to survive. Past a
    // couple, though, they are the residue of runs nobody is coming back to,
    // and nothing else reports them.
    let mut check = Check::new(Status::Ok, "container reuse", detail);
    if kept > 2 {
        check = Check::new(
            Status::Warn,
            "container reuse",
            format!("{kept} reusable containers are still up, and nothing reaps them"),
        )
        .fix("docker rm -f $(docker ps -aq --filter label=org.testcontainers.hash)");
    }
    vec![check]
}

/// The two places Testcontainers looks, in its own order: the environment
/// variable wins, then the file in the user's home directory. **Not** the
/// classpath -- `TestcontainersConfiguration` reads
/// `~/.testcontainers.properties` for this setting and a project-local file
/// is never consulted.
fn reuse_enabled() -> bool {
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
fn reusable_containers() -> usize {
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
        return Check::new(
            Status::Ok,
            "testcontainers",
            "/var/run/docker.sock is present",
        );
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
        || pom::has_dependency(
            pom_text,
            "org.springframework.boot",
            "spring-boot-starter-kafka",
        );
    let yaml = compose::read(root).unwrap_or_default();
    let has_broker = yaml.contains("# jails:kafka") || yaml.contains("\n  kafka:");
    match (has_client, has_broker) {
        (false, false) => Check::new(Status::Skip, "kafka", "not in use"),
        (true, true) => Check::new(
            Status::Ok,
            "kafka",
            "client dependency and broker service both present",
        ),
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

/// Which Jackson majors are on the classpath, and whether the java.time
/// problem is real for this project.
///
/// Two different failures live here, and they belong to different majors:
///
/// - **Jackson 2 without `jackson-datatype-jsr310`**: `findAndRegisterModules()`
///   finds no java.time support and every `LocalDate` serialises as
///   `{"year":...}` instead of an ISO string.
/// - **Both majors at once**: Boot 4's web starter brings Jackson 3
///   (`tools.jackson`), and an added 2.x `com.fasterxml` artifact sits beside
///   it quite happily -- different packages, no conflict, no warning. Half
///   the code then uses a mapper configured by nobody. This is the one that
///   is genuinely hard to see, so it outranks the other.
fn jackson_check(pom_text: &str) -> Check {
    let jackson3 = pom::has_dependency(pom_text, "tools.jackson.core", "jackson-databind");
    let jackson2 = pom::has_dependency(pom_text, "com.fasterxml.jackson.core", "jackson-databind");
    let jsr310 = pom::has_dependency(
        pom_text,
        "com.fasterxml.jackson.datatype",
        "jackson-datatype-jsr310",
    );

    if jackson3 && (jackson2 || jsr310) {
        return Check::new(
            Status::Fail,
            "json",
            "both Jackson majors are declared (tools.jackson and com.fasterxml) -- they do \
             not conflict, so nothing warns, and code written against one is configured by \
             neither",
        )
        .fix("jails remove json && jails add json   # re-adds Jackson 3 alone");
    }
    match (jackson3, jackson2, jsr310) {
        (true, _, _) => Check::new(
            Status::Ok,
            "json",
            "Jackson 3 (tools.jackson) -- java.time is built in",
        ),
        (false, false, _) => Check::new(Status::Skip, "json", "Jackson is not in use"),
        (false, true, true) => Check::new(
            Status::Warn,
            "json",
            "Jackson 2 with jackson-datatype-jsr310 -- works, but Boot 4 ships Jackson 3",
        )
        .fix("jails remove json && jails add json   # migrates to tools.jackson"),
        (false, true, false) => Check::new(
            Status::Fail,
            "json",
            "jackson-databind 2.x without jackson-datatype-jsr310 -- java.time values will \
             serialise as objects, not ISO strings",
        )
        .fix("jails add json"),
    }
}

fn port_check(root: &Path) -> Check {
    let properties = root.join("src/main/resources/application.properties");
    let configured = std::fs::read_to_string(&properties)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("server.port=")?
                    .trim()
                    .parse::<u16>()
                    .ok()
            })
        })
        .unwrap_or(8080);
    if tcp_reachable("localhost", configured, Duration::from_millis(250)) {
        Check::new(
            Status::Warn,
            "http port",
            format!(
                "something is already listening on {configured} -- a second app will fail to bind"
            ),
        )
        .fix(format!(
            "stop it, or set server.port to a free port (`lsof -i :{configured}`)"
        ))
    } else {
        Check::new(Status::Ok, "http port", format!("{configured} is free"))
    }
}

/// The static half of a "required a bean of type ... that could not be
/// found" failure, available without starting the context.
fn beans_check(root: &Path) -> Check {
    let (beans, project_types) = inspect::collect_beans(root);
    if beans.is_empty() {
        return Check::new(
            Status::Skip,
            "beans",
            "no Spring stereotypes in src/main/java",
        );
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
            format!(
                "{} bean(s), every project-typed dependency resolvable",
                beans.len()
            ),
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
        "annotate the implementation (@Component) or add an @Bean method"
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
fn running_containers() -> Vec<String> {
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
fn service_is_running(service: &str, containers: &[String]) -> bool {
    containers
        .iter()
        .any(|name| name.split(['_', '-']).any(|segment| segment == service))
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

    /// A working Jackson 2 pair still works -- it is just a version behind,
    /// so it warns rather than failing.
    #[test]
    fn both_jackson_2_artifacts_warn_rather_than_fail() {
        let pom = "<dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>\
                   <dependency><groupId>com.fasterxml.jackson.datatype</groupId>\
                   <artifactId>jackson-datatype-jsr310</artifactId></dependency>";
        assert_eq!(jackson_check(pom).status, Status::Warn);
    }

    /// The failure that loses data without an error: a DataSource exists, but
    /// the bean serving every request is a HashMap that empties on restart.
    #[test]
    fn an_in_memory_repository_bean_beside_a_datasource_is_a_failure() {
        let root = std::env::temp_dir().join(format!("jails-inmem-check-{}", std::process::id()));
        let pkg = root.join("src/main/java/com/example/demo/adapters");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("InMemoryNoteRepository.java"),
            "package com.example.demo.adapters;\n\n@Repository\npublic class InMemoryNoteRepository {}\n",
        )
        .unwrap();

        let pom = "<dependency><groupId>org.springframework.boot</groupId>\
                   <artifactId>spring-boot-starter-jdbc</artifactId></dependency>";
        let check = in_memory_adapter_check(&root, pom).expect("should report");
        assert_eq!(check.status, Status::Fail);
        assert!(
            check.detail.contains("InMemoryNoteRepository"),
            "{}",
            check.detail
        );

        // Without a DataSource it is the correct design, not a problem.
        assert!(in_memory_adapter_check(&root, "<project/>").is_none());

        // And once the annotation moves, there is nothing to report.
        std::fs::write(
            pkg.join("InMemoryNoteRepository.java"),
            "package com.example.demo.adapters;\n\npublic class InMemoryNoteRepository {}\n",
        )
        .unwrap();
        assert!(in_memory_adapter_check(&root, pom).is_none());

        std::fs::remove_dir_all(&root).ok();
    }

    /// The banner real Compose v2 prints, even when it is a CLI plugin
    /// driving podman -- which is the setup that works.
    #[test]
    fn a_real_compose_v2_banner_passes_even_under_podman() {
        let banner = ">>>> Executing external compose provider \
                      \"/home/me/.docker/cli-plugins/docker-compose\" <<<<\n\
                      Docker Compose version v5.5.0\n";
        let check = classify_compose_provider(banner);
        assert_eq!(check.status, Status::Ok);
        assert!(check.detail.contains("v5.5.0"), "{}", check.detail);
    }

    /// The failure this check exists for: the app dies during startup, before
    /// any of its own code runs, and the message names neither cause nor fix.
    #[test]
    fn a_podman_compose_provider_fails_with_the_plugin_fix() {
        let check = classify_compose_provider("podman-compose version 1.6.0\n");
        assert_eq!(check.status, Status::Fail);
        assert!(check.detail.contains("podman-compose"), "{}", check.detail);
        assert!(
            check.fix.contains("cli-plugins"),
            "the fix must be the one that leaves nothing broken: {:?}",
            check.fix
        );
    }

    #[test]
    fn jackson_3_alone_is_the_happy_path() {
        let pom = "<dependency><groupId>tools.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        assert_eq!(jackson_check(pom).status, Status::Ok);
    }

    /// The failure nothing else reports: two majors coexist quietly because
    /// their packages differ, and half the code ends up on a mapper nobody
    /// configured.
    #[test]
    fn two_jackson_majors_at_once_is_a_failure() {
        let pom = "<dependency><groupId>tools.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>\
                   <dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        let check = jackson_check(pom);
        assert_eq!(check.status, Status::Fail);
        assert!(
            check.detail.contains("both Jackson majors"),
            "{}",
            check.detail
        );
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
        let containers = vec![
            "rewards_postgres_1".to_string(),
            "other-kafka-1".to_string(),
        ];
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

// ---------------------------------------------------------------------------
// `jails setup` -- the machine-level settings a project cannot carry.
// ---------------------------------------------------------------------------

/// Turn on Testcontainers container reuse for this machine.
///
/// Everything else jails configures lives in the project, where it is visible,
/// reviewable and shared. This one cannot: Testcontainers reads
/// `testcontainers.reuse.enable` from `~/.testcontainers.properties` or the
/// environment and **never** from the classpath, so a project that asks for
/// reuse gets it only on a machine that has opted in. That asymmetry is the
/// whole reason this command exists.
///
/// **The flag alone changes nothing**, and that is deliberate. Generated
/// container configs do not call `withReuse(true)`, because the reuse key is
/// a hash of the container's configuration and nothing in it identifies the
/// project -- two applications on the same image would share one database,
/// and Flyway would refuse to start against the other one's migration
/// history. This command sets up the half a machine owns; the project half is
/// a one-line change the reader makes deliberately, and `TestcontainersConfig`
/// says so in its Javadoc.
///
/// The edit is a splice, not a rewrite: `~/.testcontainers.properties` is a
/// file the reader owns and may already hold `docker.client.strategy`,
/// `ryuk.disabled` or a registry mirror. Same rule as `pom.xml`.
pub fn setup(dry_run: bool) -> Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err("no HOME, so there is no ~/.testcontainers.properties to write".to_string());
    };
    let path = Path::new(&home).join(".testcontainers.properties");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    if existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .any(|line| line.replace(' ', "").starts_with("testcontainers.reuse.enable="))
    {
        // Present already -- including as `=false`, which is a decision, not
        // an omission. Flipping someone's explicit `false` would be jails
        // overruling them on their own machine.
        println!(
            "  exists  testcontainers.reuse.enable is already set in {}",
            path.display()
        );
        println!("          jails doctor reports whether it is on");
        return Ok(());
    }

    let mut next = existing.clone();
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(REUSE_BLOCK);

    if dry_run {
        println!("would add to {}:", path.display());
        for line in REUSE_BLOCK.lines() {
            println!("  {line}");
        }
        println!();
        println!("--dry-run: nothing was written.");
        return Ok(());
    }

    std::fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    println!(
        "  write   testcontainers.reuse.enable=true -> {}",
        path.display()
    );
    println!("          This machine now permits reuse. Nothing reuses anything yet:");
    println!("          add `.withReuse(true)` to the container bean in TestcontainersConfig,");
    println!("          and read its Javadoc first -- two projects on one image share a");
    println!("          database, and Flyway will not start against another project's history.");
    println!("          Reused containers are not reaped; `jails doctor` counts them.");
    Ok(())
}

const REUSE_BLOCK: &str = "\
# jails: permit containers to be reused between test runs -- the largest
# saving available to a suite that starts PostgreSQL.
#
# This only permits it. A container is reused when its bean asks, with
# `withReuse(true)`, and that is a per-project decision: the reuse key is a
# hash of the container configuration, so two projects on the same image would
# share one database and Flyway would reject the other one's migrations.
#
# Reused containers are deliberately not registered with Ryuk, so nothing
# reaps them -- `jails doctor` counts them.
testcontainers.reuse.enable=true
";
