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

use super::{Check, Status};
use crate::compose;
use crate::model::Project;
use crate::pom;
use std::path::Path;

mod storage;

pub(super) use storage::{database_checks, in_memory_adapter_check, sql_init_checks};
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
    let has_broker = compose::declares(&yaml, "kafka") || yaml.contains("\n  kafka:");
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
///
/// The guard asks [`Project::has_jdbc`] directly. It used to ask the legacy
/// planner, so that `doctor` could not hold a second opinion about what "has a
/// database" means -- but the planner's answer *was* this method, three hops
/// down, and the compiler decides the same arm from the model instead. Asking
/// the project is now the shortest route to the one opinion there is.
pub(super) fn duplicate_key_check(project: &Project) -> Check {
    if !project.has_jdbc() {
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
            .fix(
                "jails add dependency org.springframework.boot:spring-boot-devtools --scope \
                 runtime",
            ),
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
