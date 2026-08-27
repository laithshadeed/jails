//! The checks that ask the *project* whether a capability is wired up.
//!
//! A dependency is present but the property that makes it work is not; two
//! Jackson majors are on one classpath and nothing warns; a `@SpringBootTest`
//! has no `@Import(TestcontainersConfig.class)`, so JDBC auto-config fails on a
//! test nobody wrote.
//!
//! These are deliberately **not** derived from `add::plan_for`, which
//! `doctor::capability_drift_checks` does do. A derived check knows a
//! dependency is missing; it does not know that two Jackson majors is a silent
//! disaster, or that a `spring.factories` left behind starts a second container
//! for every test. Those are interaction facts no plan carries, and
//! `abstract.md` §6.2 says exactly that.

use super::environment::tcp_reachable;
use super::{Check, Status};
use crate::compose;
use crate::model::Project;
use crate::pom;
use std::path::Path;
use std::time::Duration;

pub(super) fn database_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
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
pub(super) fn imports_config(text: &str, class: &str) -> bool {
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

/// The JDBC container types Spring maps to a `DataSource`.
///
/// A closed list rather than "anything ending in Container", because that
/// would read a Kafka, Redis or Toxiproxy container as a database and put the
/// project's datasource check on the wrong class. jails itself only ever
/// writes PostgreSQL; the rest are here because `doctor` runs on projects
/// jails did not create.
const JDBC_CONTAINERS: &[&str] = &[
    "PostgreSQLContainer",
    "MySQLContainer",
    "MariaDBContainer",
    "OracleContainer",
    "MSSQLServerContainer",
    "JdbcDatabaseContainer",
];

pub(super) fn test_container_wiring(root: &Path) -> (Option<String>, Vec<String>) {
    let tests = root.join("src/test/java");
    // The config this check is about, and only that one. `add kafka` writes a
    // `@TestConfiguration` with `@ServiceConnection` too, and taking whichever
    // the walk saw last made `doctor` report every `@SpringBootTest` in the
    // project as missing an import of `KafkaTestcontainersConfig` -- under the
    // heading "test datasource", with `jails add db` as the fix, on a project
    // where `add db` was installed and correct.
    //
    // The discriminator is the container's *type*, because the invariant this
    // check exists for is specific to JDBC: once `spring-boot-starter-jdbc` is
    // present, auto-configuration demands a `DataSource` for every
    // `@SpringBootTest`, including ones that never touch a database. A broker
    // has no equivalent demand.
    let config = crate::java::types_annotated_with(&tests, "TestConfiguration")
        .into_iter()
        .filter(|found| {
            crate::java::annotations(&found.source)
                .iter()
                .any(|annotation| annotation.name == "ServiceConnection")
                && crate::java::declares_any_type(&found.source, JDBC_CONTAINERS)
        })
        .filter_map(|found| found.type_name().map(str::to_string))
        .next_back();

    let unimported = match &config {
        Some(class) => crate::java::types_annotated_with(&tests, "SpringBootTest")
            .into_iter()
            .filter_map(|found| {
                let stem = found.type_name()?.to_string();
                // The config class does not import itself.
                (stem != *class && !imports_config(&found.source, class)).then_some(stem)
            })
            .collect(),
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
pub(super) fn in_memory_adapter_check(project: &Project) -> Option<Check> {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
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

pub(super) fn kafka_check(project: &Project) -> Check {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
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
pub(super) fn jackson_check(project: &Project) -> Check {
    // Through the project, so the answer comes from whichever build file this
    // is. Parsing a `build.gradle` as XML reports every Jackson artifact
    // absent, which reads as "not in use" directly above a capability check
    // saying it is installed.
    let jackson3 =
        project.declares_dependency("tools.jackson.core", "jackson-databind") == Some(true);
    let jackson2 =
        project.declares_dependency("com.fasterxml.jackson.core", "jackson-databind") == Some(true);
    let jsr310 = project
        .declares_dependency("com.fasterxml.jackson.datatype", "jackson-datatype-jsr310")
        == Some(true);

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

/// A `@unique` violation answers 409, not 500.
///
/// **`pending.md` §1.1.** jails puts `@unique` in the schema and generates an
/// `ApiException.Conflict` documented "Becomes a 409", and for a long time
/// nothing connected the two: inserting a duplicate reached the client as a
/// **500**, which is what alerting pages on and what client libraries retry.
/// One duplicate became an incident and then a retry storm.
///
/// `add api` renders the `DuplicateKeyException` arm when the JDBC starter is
/// present, so `add db api`, `add db` then `add api`, and any `app apply`
/// declaring both are all correct. What this catches is the other order --
/// `add api` first, `add db` later -- where the advice on disk describes a
/// project without a database, because a capability's plan is a pure function
/// of the project at the moment it was applied.
///
/// That order is not a defect to be prevented; it is the ordinary way somebody
/// grows a project, and `jails sync` re-plans every recorded capability and
/// applies the difference. What was missing is anything that *says so*. This
/// is that.
///
/// It reads the file rather than the ledger deliberately. The question is what
/// the running application does with a duplicate, and that is decided by the
/// bytes on disk -- including bytes the reader wrote themselves, which is why
/// a handler they have taught to answer 409 by hand passes.
pub(super) fn duplicate_key_check(project: &Project) -> Check {
    if !crate::add::plan_supports_duplicate_keys(project) {
        return Check::new(
            Status::Skip,
            "conflicts",
            "no JDBC starter -- nothing enforces a unique constraint",
        );
    }
    let Some(handler) = api_advice(project) else {
        return Check::new(
            Status::Skip,
            "conflicts",
            "no ApiExceptionHandler -- `jails add api` writes the advice a 409 comes from",
        );
    };
    match std::fs::read_to_string(&handler) {
        Ok(source) if source.contains("DuplicateKeyException") => Check::new(
            Status::Ok,
            "conflicts",
            "a duplicate key answers 409 rather than 500",
        ),
        Ok(_) => Check::new(
            Status::Fail,
            "conflicts",
            "this project has a database and unique constraints, and its ApiExceptionHandler \
             does not map DuplicateKeyException -- a duplicate answers 500, which alerting \
             pages on and clients retry",
        )
        .fix("jails sync"),
        Err(error) => Check::new(
            Status::Warn,
            "conflicts",
            format!("{} is unreadable: {error}", handler.display()),
        )
        .fix("check the file's permissions"),
    }
}

/// Where `add api` put the advice, honouring a `jails.toml` layer rename.
fn api_advice(project: &Project) -> Option<std::path::PathBuf> {
    let package = project.package_named(jails_spec::spec::layout::API, None);
    let path = project
        .root()
        .join("src/main/java")
        .join(package.replace('.', "/"))
        .join("ApiExceptionHandler.java");
    path.is_file().then_some(path)
}

/// Static safety checks for an actuator endpoint set. These are warnings,
/// not startup failures: the application will run with all three mistakes,
/// which is exactly why they belong in `doctor`.
pub(super) fn management_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
    if !pom::has_dependency(
        pom_text,
        "org.springframework.boot",
        "spring-boot-starter-actuator",
    ) {
        return Vec::new();
    }
    let path = root.join("src/main/resources/application.properties");
    let properties = std::fs::read_to_string(path).unwrap_or_default();
    let application_port = property_value(&properties, "server.port").unwrap_or("8080");
    let management_port = property_value(&properties, "management.server.port");
    let mut checks = Vec::new();

    checks.push(match management_port {
        Some(port) if !port.is_empty() && port != application_port => Check::new(
            Status::Ok,
            "management port",
            format!("isolated on {port} (application port {application_port})"),
        ),
        _ => Check::new(
            Status::Warn,
            "management port",
            "Actuator shares the public connector and thread pool; traffic pressure can starve probes",
        )
        .fix("jails add actuator (idempotent -- sets management.server.port=8081)"),
    });

    let exposure = property_value(&properties, "management.endpoints.web.exposure.include")
        .unwrap_or("health");
    let dangerous: Vec<&str> = exposure
        .split(',')
        .map(str::trim)
        .filter(|name| matches!(*name, "*" | "env" | "configprops" | "heapdump"))
        .collect();
    checks.push(if dangerous.is_empty() {
        Check::new(
            Status::Ok,
            "management exposure",
            format!("explicit endpoint allow-list: {exposure}"),
        )
    } else {
        Check::new(
            Status::Warn,
            "management exposure",
            format!(
                "credential- or memory-bearing endpoint(s) exposed: {}",
                dangerous.join(", ")
            ),
        )
        .fix("replace exposure.include with health,info,prometheus,threaddump")
    });

    let liveness = property_value(
        &properties,
        "management.endpoint.health.group.liveness.include",
    );
    checks.push(match liveness {
        Some(value) if value.split(',').all(|name| name.trim() == "ping") => Check::new(
            Status::Ok,
            "liveness group",
            "process-only (`ping`); dependency outages cannot trigger pod restarts",
        ),
        Some(value) => Check::new(
            Status::Warn,
            "liveness group",
            format!(
                "contains dependency indicators ({value}); a transient outage can make Kubernetes kill healthy pods"
            ),
        )
        .fix("set management.endpoint.health.group.liveness.include=ping"),
        None => Check::new(
            Status::Warn,
            "liveness group",
            "not explicit; keep liveness process-only and put dependencies in readiness",
        )
        .fix("jails add actuator (idempotent -- writes explicit probe groups)"),
    });
    checks
}

pub(super) fn cors_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
    // Either spelling. Boot 4 renamed the starter and deprecated the old
    // name, but `spring-boot-starter-web` is what every project written
    // before that says -- and those are exactly the projects being adopted.
    // Matching only the new name is how this check came to report nothing on
    // `minicom-15-01-2026`, whose `@EnableWebMvc` was silently discarding
    // every `spring.jackson.*` property it had.
    if !pom_text.contains("spring-boot-starter-webmvc")
        && !pom_text.contains("spring-boot-starter-web")
    {
        return Vec::new();
    }
    let mut enable_webmvc = Vec::new();
    let mut wildcard_without_origins = Vec::new();
    for path in crate::java::source_files(&root.join("src/main/java")) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if source.contains("@EnableWebMvc") {
            enable_webmvc.push(path.display().to_string());
        }
        if source.contains("addMapping(\"/**\")")
            && !source.contains("allowedOrigins(")
            && !source.contains("allowedOriginPatterns(")
            && !source.contains("setAllowedOrigins(")
        {
            wildcard_without_origins.push(path.display().to_string());
        }
    }
    let mut checks = Vec::new();
    if !enable_webmvc.is_empty() {
        checks.push(
            Check::new(
                Status::Warn,
                "MVC override",
                format!(
                    "@EnableWebMvc disables Boot MVC auto-configuration in {} -- every \
                     spring.jackson.* property is ignored, and so is every converter Boot \
                     would have contributed",
                    enable_webmvc.join(", ")
                ),
            )
            .fix(
                "remove @EnableWebMvc; a WebMvcConfigurer bean still customises MVC, and \
                  keeps the auto-configuration",
            ),
        );
    }
    if !wildcard_without_origins.is_empty() {
        checks.push(
            Check::new(
                Status::Warn,
                "CORS origins",
                format!(
                    "global /** mapping has no explicit origin allow-list in {}",
                    wildcard_without_origins.join(", ")
                ),
            )
            .fix("jails add cors, then set app.cors.allowed-origins"),
        );
    }
    checks
}

pub(super) fn property_value<'a>(properties: &'a str, key: &str) -> Option<&'a str> {
    properties.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then_some(value.trim())
    })
}

pub(super) fn virtual_thread_checks(root: &Path) -> Vec<Check> {
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();
    if property_value(&properties, "spring.threads.virtual.enabled") != Some("true") {
        return Vec::new();
    }

    let mut scheduled = Vec::new();
    let mut synchronised = Vec::new();
    for path in crate::java::source_files(&root.join("src/main/java")) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let label = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        if source.contains("@Scheduled") {
            scheduled.push(label.clone());
        }
        if crate::java::blanked(&source).contains("synchronized") {
            synchronised.push(label);
        }
    }

    let mut checks = Vec::new();
    if !scheduled.is_empty()
        && property_value(&properties, "spring.main.keep-alive") != Some("true")
    {
        checks.push(
            Check::new(
                Status::Warn,
                "virtual keep-alive",
                format!(
                    "virtual threads plus @Scheduled can let the JVM exit cleanly when no platform thread remains ({})",
                    scheduled.join(", ")
                ),
            )
            .fix("set spring.main.keep-alive=true"),
        );
    }
    if !synchronised.is_empty() {
        checks.push(
            Check::new(
                Status::Warn,
                "virtual pinning",
                format!(
                    "synchronized code may pin carrier threads in {}; measure the jdk.VirtualThreadPinned JFR event",
                    synchronised.join(", ")
                ),
            )
            .fix("jcmd <pid> JFR.start name=jails settings=profile duration=60s filename=target/virtual-threads.jfr"),
        );
    }
    checks
}

/// Whether a save in the editor actually reaches the running application.
///
/// `plan.md` §19.5 asked where jdt.ls writes `.class` files here, because
/// §10.3's whole `jails dev` supervisor was conditional on the answer. It is
/// **measured now**, not assumed: a fresh `jails new-cli` project with no
/// `target/` at all, opened headless in nvim and left alone until class files
/// appeared, produced `target/classes/**.class` and
/// `target/test-classes/**.class` with **no Maven run**. m2e points Eclipse's
/// output folder at Maven's own, which is the premise §10.3 needed.
///
/// So the loop already exists, and jails already ships both halves of it:
/// jdt.ls compiles on `:w`, devtools polls the classpath and restarts, and
/// `jails new` writes `META-INF/spring-devtools.properties` to cut Boot's
/// 1 s + 400 ms of waiting down to 200 ms + 50 ms. Nothing here needs a file
/// watcher, a `javac` invocation or a JDWP client.
///
/// What was missing was not machinery but a way to find out it is broken --
/// and every way it breaks is **silent**. Each check below is a property
/// whose wrong value costs nothing at startup and simply means saving a file
/// does nothing, which reads as "hot reload doesn't work here" rather than as
/// a setting somebody chose.
pub(super) fn hot_reload_checks(project: &Project) -> Vec<Check> {
    let pom_text = project.pom();
    if !pom::has_dependency(
        pom_text,
        "org.springframework.boot",
        "spring-boot-starter-parent",
    ) && !pom::has_dependency(
        pom_text,
        "org.springframework.boot",
        "spring-boot-starter-web",
    ) {
        return Vec::new();
    }
    let root = project.root();
    let mut checks = Vec::new();

    if !pom::has_dependency(pom_text, "org.springframework.boot", "spring-boot-devtools") {
        checks.push(
            Check::new(
                Status::Warn,
                "reload",
                "no spring-boot-devtools: the editor recompiles into target/classes on save, but the running application never picks it up",
            )
            .fix("add org.springframework.boot:spring-boot-devtools with <optional>true</optional>"),
        );
        return checks;
    }

    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();

    if property_value(&properties, "spring.devtools.restart.enabled") == Some("false") {
        checks.push(
            Check::new(
                Status::Fail,
                "reload",
                "spring.devtools.restart.enabled=false: devtools is a dependency but restarts are switched off, so saving a file changes nothing",
            )
            .fix("remove spring.devtools.restart.enabled from src/main/resources/application.properties"),
        );
    } else if let Some(trigger) =
        property_value(&properties, "spring.devtools.restart.trigger-file")
    {
        // The trap this exists for: with a trigger file set, a recompiled
        // class is *seen* and deliberately ignored until that one file is
        // touched. Nothing logs the decision, so the loop looks dead.
        checks.push(
            Check::new(
                Status::Warn,
                "reload",
                format!(
                    "spring.devtools.restart.trigger-file={trigger}: a saved class will not restart the application until that file is touched"
                ),
            )
            .fix(format!("touch {trigger} after a save, or remove the property")),
        );
    } else {
        let tuned = root.join("src/main/resources/META-INF/spring-devtools.properties");
        let tuned_text = std::fs::read_to_string(&tuned).unwrap_or_default();
        checks.push(if tuned_text.contains("restart.poll-interval") {
            Check::new(
                Status::Ok,
                "reload",
                "save in the editor recompiles into target/classes and devtools restarts (polling tuned to 200ms/50ms)",
            )
        } else {
            Check::new(
                Status::Warn,
                "reload",
                "devtools is using Boot's 1s poll and 400ms quiet period, so a save waits up to 1.4s before the restart even begins",
            )
            .fix("jails new writes src/main/resources/META-INF/spring-devtools.properties with defaults.spring.devtools.restart.poll-interval=200ms and quiet-period=50ms")
        });
    }

    checks
}

/// `spring.sql.init.mode` is set and there is a schema for it to run.
///
/// The failure this catches is silent by design of Spring's own defaults: with
/// `spring.sql.init.mode=always` and no readable `schema.sql`, the context
/// starts perfectly. The tables are simply absent, and the first query to need
/// one fails in front of a user.
///
/// Deliberately **not** a SQL parser. jails does not read SQL and should not
/// start: what it can say exactly is whether the mechanism was switched on and
/// left with nothing to run, which is the whole of the reported failure.
pub(super) fn sql_init_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();
    let Some(mode) = property_value(&properties, "spring.sql.init.mode") else {
        return Vec::new();
    };
    if mode == "never" {
        return Vec::new();
    }
    // Spring reads `schema.sql` and `data.sql` from the classpath root, and
    // either alone is enough to make the setting mean something.
    let present: Vec<&str> = ["schema.sql", "data.sql"]
        .into_iter()
        .filter(|name| root.join("src/main/resources").join(name).is_file())
        .collect();
    if present.is_empty() {
        return vec![
            Check::new(
                Status::Fail,
                "sql init",
                format!(
                    "spring.sql.init.mode={mode}, and there is no schema.sql or data.sql to \
                     run. The context still starts -- the tables are simply absent, and the \
                     first query to need one fails in front of a user"
                ),
            )
            .fix("add src/main/resources/schema.sql, or set spring.sql.init.mode=never"),
        ];
    }
    vec![Check::new(
        Status::Ok,
        "sql init",
        format!(
            "spring.sql.init.mode={mode}, running {}",
            present.join(" and ")
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, at: &str, body: &str) {
        let path = root.join(at);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// The check is about the datasource, and a broker's container config is
    /// not one.
    ///
    /// `add db` and `add kafka` both write a `@TestConfiguration` carrying
    /// `@ServiceConnection`. Taking whichever the directory walk saw last made
    /// `doctor` report every `@SpringBootTest` in the project as missing an
    /// import of `KafkaTestcontainersConfig`, under the heading "test
    /// datasource", and offer `jails add db` as the fix -- on a project where
    /// `add db` was already installed and correct.
    #[test]
    fn the_datasource_check_ignores_a_broker_container_config() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-wiring-kafka")
            .unwrap()
            .keep();
        write(
            &root,
            "src/test/java/com/example/TestcontainersConfig.java",
            "package com.example;\n\n@TestConfiguration\nclass TestcontainersConfig {\n    \
             @Bean\n    @ServiceConnection\n    PostgreSQLContainer postgres() { return null; }\n}\n",
        );
        write(
            &root,
            "src/test/java/com/example/KafkaTestcontainersConfig.java",
            "package com.example;\n\n@TestConfiguration\nclass KafkaTestcontainersConfig {\n    \
             @Bean\n    @ServiceConnection\n    KafkaContainer broker() { return null; }\n}\n",
        );
        write(
            &root,
            "src/test/java/com/example/DemoApplicationTests.java",
            "package com.example;\n\n@SpringBootTest\n@Import(TestcontainersConfig.class)\n\
             class DemoApplicationTests {}\n",
        );

        let (config, unimported) = test_container_wiring(&root);

        assert_eq!(config.as_deref(), Some("TestcontainersConfig"));
        assert!(
            unimported.is_empty(),
            "the test imports the datasource config it needs: {unimported:?}"
        );
    }

    /// A container named only in a Javadoc example does not make the class
    /// that mentions it a datasource config.
    #[test]
    fn a_container_named_in_a_comment_is_not_one_the_class_declares() {
        assert!(!crate::java::declares_any_type(
            "// use PostgreSQLContainer here\nclass Nothing {}\n",
            JDBC_CONTAINERS
        ));
        assert!(crate::java::declares_any_type(
            "class Real { PostgreSQLContainer c; }\n",
            JDBC_CONTAINERS
        ));
    }
}
