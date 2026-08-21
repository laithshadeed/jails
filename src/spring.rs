//! Spring-only capabilities: the slices that only make sense inside a Spring
//! Boot application.
//!
//! These live apart from `add.rs` for two reasons. `add.rs` is already the
//! biggest file in the project, and everything here shares one precondition
//! -- a Spring Boot parent -- which is checked in one place rather than
//! re-derived per capability.
//!
//! Every template here was written against the sources under `deps/`
//! (Spring Boot 4.x, Spring Framework 7.x), not from memory. Where an API
//! moved or was replaced, the comment says which one and why, because the
//! failure mode for generated code is silent: it compiles against the
//! version you had and breaks on the version the reader has.

use std::path::Path;

use crate::Result;
use crate::pom::{Dependency, Flavor};

/// A capability's contribution, in the shape `add.rs` already understands.
pub(crate) struct SpringSlice {
    pub deps: Vec<Dependency>,
    pub files: Vec<(std::path::PathBuf, String)>,
    /// Lines to splice into `application.properties`, each with the marker
    /// comment that lets `remove` take them back out.
    pub properties: Vec<String>,
}

pub(crate) const VALIDATION_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-validation",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const ACTUATOR_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-actuator",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const CACHE_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-cache",
    version: None,
    scope: None,
    optional: false,
};

/// What actually applies `spring.http.serviceclient.<group>.*` to a
/// declarative client.
///
/// Easy to miss, and the failure is confusing rather than loud: without this
/// module `@ImportHttpServices` still builds the proxies (that part is
/// Framework, not Boot), but nothing binds the group's base URL, so the first
/// call fails with "URI with undefined scheme" -- a message that says nothing
/// about a missing dependency. `spring-boot-starter-webmvc` does not bring it;
/// serving HTTP and calling it are separate concerns.
pub(crate) const RESTCLIENT_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-restclient",
    version: None,
    scope: None,
    optional: false,
};

/// Apache HttpClient is used by the safe fetcher because its DNS resolver can
/// be pinned to the addresses that passed policy validation. JDK HttpClient
/// does not expose that boundary, leaving a DNS-rebinding window between
/// validation and connection.
pub(crate) const APACHE_HTTPCLIENT: Dependency = Dependency {
    group_id: "org.apache.httpcomponents.client5",
    artifact_id: "httpclient5",
    version: None,
    scope: None,
    optional: false,
};

/// Caffeine is the cache Spring Boot picks up automatically when it is on
/// the classpath and nothing else claims the slot. Version managed by the
/// Boot parent so it moves with the platform.
pub(crate) const CAFFEINE: Dependency = Dependency {
    group_id: "com.github.ben-manes.caffeine",
    artifact_id: "caffeine",
    version: None,
    scope: None,
    optional: false,
};

/// Refuse politely rather than generating Spring code into a plain Maven
/// project, where it would not compile and the reason would not be obvious.
pub(crate) fn require_spring(flavor: Flavor, capability: &str) -> Result<()> {
    match flavor {
        Flavor::SpringBoot => Ok(()),
        Flavor::PlainMaven => Err(format!(
            "`{capability}` is a Spring Boot capability, and this is a plain Maven project.\n       \
             `jails new <name>` creates a Spring project; `jails add http` is the framework-free \
             HTTP option."
        )),
    }
}

// ---------------------------------------------------------------------------
// `add api` -- one place where failures become HTTP responses.
// ---------------------------------------------------------------------------

/// The error-handling slice.
///
/// This is the single largest piece of boilerplate in a Spring web service,
/// and the one most often written slightly differently in every project: a
/// controller that catches its own exceptions, a `Map<String, String>` error
/// body invented per endpoint, validation failures surfacing as a 500
/// because nothing handled them.
///
/// What replaces it is RFC 9457 (`application/problem+json`), which Spring
/// models as `ProblemDetail` and already produces for its own exceptions.
/// The generated advice extends `ResponseEntityExceptionHandler` -- Spring's
/// own base class, so every framework exception keeps its correct status and
/// only the project's own exceptions need mapping.
pub(crate) fn api_slice(root: &Path, pkg: &str) -> SpringSlice {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![VALIDATION_STARTER],
        files: vec![
            (main.join("ApiException.java"), api_exception_java(pkg)),
            (
                main.join("ApiExceptionHandler.java"),
                api_exception_handler_java(pkg),
            ),
            (
                test.join("ApiExceptionHandlerTest.java"),
                api_exception_handler_test_java(pkg),
            ),
        ],
        properties: Vec::new(),
    }
}

/// The project's own failures, as a sealed set.
///
/// Sealed rather than an open hierarchy: the handler switches over these, and
/// a `default` branch is what lets a new failure type silently become a 500.
/// With `permits` spelled out, adding one breaks the build at the switch --
/// which is where the decision about its status code belongs.
fn api_exception_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/api_exception_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/api_exception_handler_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/api_exception_handler_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `add actuator` -- health, metrics and info, without inventing endpoints.
// ---------------------------------------------------------------------------

/// The actuator exposure list, unioned with whatever is already set.
///
/// Two capabilities own this one key -- `actuator` and `observability` -- and
/// each installs its properties as its own marked block. Properties are
/// last-wins, so whichever was added second would otherwise silently narrow
/// the other: `add observability` then `add actuator` leaves `prometheus`
/// unexposed and the scrape returns 404 with nothing in the logs to say why.
/// Reading the current value and unioning makes the order stop mattering.
fn exposure_include(root: &Path, wanted: &[&str]) -> String {
    let mut names: Vec<String> = Vec::new();
    let path = root.join("src/main/resources/application.properties");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        for line in existing.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("management.endpoints.web.exposure.include=") {
                for name in value.split(',') {
                    let name = name.trim();
                    // `*` is a wildcard, not a name: keep it and stop, rather
                    // than expanding a user's deliberate choice into a list.
                    if !name.is_empty() && !names.iter().any(|n| n == name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    for name in wanted {
        if !names.iter().any(|n| n == name) {
            names.push((*name).to_string());
        }
    }
    format!(
        "management.endpoints.web.exposure.include={}",
        names.join(",")
    )
}

pub(crate) fn actuator_slice(root: &Path, pkg: &str) -> SpringSlice {
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![ACTUATOR_STARTER],
        files: vec![(
            test.join("ActuatorEndpointsTest.java"),
            actuator_test_java(pkg, crate::generate::mockmvc_autoconfigure_import(root)),
        )],
        // Exposed deliberately and narrowly. The default over HTTP is health
        // alone; `*` is the shape that leaks heap dumps and environment
        // variables to anything that can reach the port.
        properties: vec![
            exposure_include(root, &["health", "info", "metrics"]),
            "management.endpoint.health.show-details=when-authorized".to_string(),
        ],
    }
}

fn actuator_test_java(pkg: &str, mockmvc_import: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/actuator_test_java.java"),
        &[("pkg", pkg), ("mockmvc_import", mockmvc_import)],
    )
}

// ---------------------------------------------------------------------------
// `add cache` -- caching that is switched on and provably working.
// ---------------------------------------------------------------------------

pub(crate) fn cache_slice(root: &Path, pkg: &str) -> SpringSlice {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![CACHE_STARTER, CAFFEINE],
        files: vec![
            (main.join("CacheConfig.java"), cache_config_java(pkg)),
            (test.join("CacheConfigTest.java"), cache_test_java(pkg)),
        ],
        properties: vec![
            "spring.cache.type=caffeine".to_string(),
            // A cache with no bound is a memory leak with a friendly name.
            "spring.cache.caffeine.spec=maximumSize=1000,expireAfterWrite=60s".to_string(),
        ],
    }
}

fn cache_config_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/cache_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn cache_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/cache_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `generate client` -- a declarative HTTP client.
// ---------------------------------------------------------------------------

/// The files for `jails generate client <Name>`.
///
/// Spring Boot 4 registers `@HttpExchange` interfaces itself, given
/// `@ImportHttpServices`, and binds each group's base URL to
/// `spring.http.serviceclient.<group>.base-url`. That combination replaces
/// the usual hand-written client: no `RestTemplate` field, no URI building,
/// no response-entity unwrapping, and the base URL is configuration rather
/// than a constant compiled into the jar.
pub(crate) fn client_files(
    root: &Path,
    pkg: &str,
    name: &str,
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let group = crate::sql::snake_case(name).replace('_', "-");
    vec![
        (
            main.join(format!("{name}Client.java")),
            client_interface_java(pkg, name),
            "http client",
        ),
        (
            main.join("HttpClientsConfig.java"),
            client_config_java(pkg, &group),
            "http client registration",
        ),
        (
            test.join(format!("{name}ClientTest.java")),
            client_test_java(pkg, name, &group),
            "http client test",
        ),
    ]
}

fn client_interface_java(pkg: &str, name: &str) -> String {
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    format!(
        r#"package {pkg};

import java.util.List;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.service.annotation.GetExchange;

/**
 * The {name} service, as this application uses it.
 *
 * <p>An interface and nothing else: Spring builds the implementation. There is
 * no base URL here on purpose -- the client belongs to a group (see
 * {{@link HttpClientsConfig}}) whose URL comes from
 * {{@code spring.http.serviceclient.*.base-url}}, so pointing the client at a
 * stub, a staging host or production is configuration rather than a code
 * change.
 *
 * <p>Return domain types, not {{@code ResponseEntity}}: a non-2xx response
 * already becomes an exception, so unwrapping one by hand at every call site
 * buys nothing.
 */
public interface {name}Client {{

    /** @return every item the upstream service knows about. */
    @GetExchange("{path}")
    List<{name}Payload> findAll();

    /** @return one item by id. A 404 upstream surfaces as an exception. */
    @GetExchange("{path}/{{id}}")
    {name}Payload findById(@PathVariable String id);

    /**
     * What the upstream service returns. A record of its own rather than a
     * domain type: the shape belongs to them and will change on their
     * schedule, and letting it reach the domain directly is how an external
     * rename becomes a refactor here.
     */
    record {name}Payload(String id, String name) {{}}
}}
"#
    )
}

fn client_config_java(pkg: &str, group: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/client_config_java.java"),
        &[("pkg", pkg), ("group", group)],
    )
}

fn client_test_java(pkg: &str, name: &str, group: &str) -> String {
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    format!(
        r#"package {pkg};

import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.DynamicPropertyRegistry;
import org.springframework.test.context.DynamicPropertySource;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Drives the client against a real HTTP server on an ephemeral port.
 *
 * <p>The stub is the JDK's own {{@link HttpServer}} -- no extra dependency, and
 * a real socket, so this exercises serialization, status handling and the
 * configured base URL rather than a mock's idea of them.
 *
 * <p>{{@code @DynamicPropertySource}} is what makes the port work: it is
 * resolved after the server binds but before the context starts, which no
 * static property file can do.
 */
@SpringBootTest
class {name}ClientTest {{

    private static HttpServer server;

    @Autowired
    private {name}Client client;

    @BeforeAll
    static void startStub() throws IOException {{
        // Port 0: the OS picks a free one, so parallel runs cannot collide.
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext("{path}", exchange -> {{
            byte[] body = body(exchange.getRequestURI().getPath()).getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().add("Content-Type", "application/json");
            exchange.sendResponseHeaders(200, body.length);
            try (var out = exchange.getResponseBody()) {{
                out.write(body);
            }}
        }});
        server.start();
    }}

    @AfterAll
    static void stopStub() {{
        server.stop(0);
    }}

    @DynamicPropertySource
    static void baseUrl(DynamicPropertyRegistry registry) {{
        registry.add(
                "spring.http.serviceclient.{group}.base-url",
                () -> "http://localhost:" + server.getAddress().getPort());
    }}

    private static String body(String path) {{
        return path.equals("{path}")
                ? "[{{\"id\":\"1\",\"name\":\"first\"}}]"
                : "{{\"id\":\"1\",\"name\":\"first\"}}";
    }}

    @Test
    void findAllReadsTheCollection() {{
        assertThat(client.findAll()).containsExactly(new {name}Client.{name}Payload("1", "first"));
    }}

    @Test
    void findByIdReadsOneItem() {{
        assertThat(client.findById("1").name()).isEqualTo("first");
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// `generate fetcher` -- bounded, SSRF-safe outbound bytes.
// ---------------------------------------------------------------------------

pub(crate) fn fetcher_files(
    root: &Path,
    pkg: &str,
    name: &str,
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let property = crate::sql::snake_case(name).replace('_', "-");
    vec![
        (
            main.join(format!("{name}Fetcher.java")),
            crate::template::render(
                include_str!("../templates/spring/fetcher_port_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
            "safe fetch port",
        ),
        (
            main.join(format!("Safe{name}Fetcher.java")),
            crate::template::render(
                include_str!("../templates/spring/safe_fetcher_java.java"),
                &[("pkg", pkg), ("name", name), ("property", &property)],
            ),
            "safe fetch adapter",
        ),
        (
            test.join(format!("Safe{name}FetcherTest.java")),
            crate::template::render(
                include_str!("../templates/spring/safe_fetcher_test_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
            "safe fetch adversarial test",
        ),
    ]
}

// ---------------------------------------------------------------------------
// `generate job` -- scheduled work.
// ---------------------------------------------------------------------------

pub(crate) fn job_files(
    root: &Path,
    pkg: &str,
    name: &str,
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    vec![
        (
            main.join(format!("{name}Job.java")),
            job_java(pkg, name),
            "job",
        ),
        (
            main.join("SchedulingConfig.java"),
            scheduling_config_java(pkg),
            "scheduling",
        ),
        (
            test.join(format!("{name}JobTest.java")),
            job_test_java(pkg, name),
            "job test",
        ),
    ]
}

fn job_java(pkg: &str, name: &str) -> String {
    let property = crate::sql::snake_case(name).replace('_', "-");
    format!(
        r#"package {pkg};

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/**
 * Scheduled work, with the schedule itself left in configuration.
 *
 * <p>{{@code fixedDelayString}} reads {{@code jobs.{property}.delay}} rather
 * than hard-coding an interval, so a test can run it every 10ms and production
 * every ten minutes without touching this file.
 *
 * <p>{{@code fixedDelay}} and not {{@code fixedRate}}: the delay is measured
 * from the end of the previous run, so a slow run delays the next one instead
 * of queueing another on top of it. Reach for {{@code fixedRate}} only when
 * you genuinely want overlapping executions.
 *
 * <p>The body catches its own failures. An exception escaping a scheduled
 * method kills the schedule for the rest of the JVM's life, silently -- which
 * is a strange default and the most common way a job stops running without
 * anyone noticing.
 */
@Component
public class {name}Job {{

    private static final Logger log = LoggerFactory.getLogger({name}Job.class);

    @Scheduled(fixedDelayString = "${{jobs.{property}.delay:PT1M}}")
    public void run() {{
        try {{
            work();
        }} catch (RuntimeException failure) {{
            // Swallowed deliberately: rethrowing here cancels all future runs.
            log.error("{name}Job failed; the schedule continues", failure);
        }}
    }}

    /** The actual work. Package-private so a test can call it directly. */
    void work() {{
        log.info("{name}Job ran");
    }}
}}
"#
    )
}

fn scheduling_config_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/scheduling_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn job_test_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/job_test_java.java"),
        &[("pkg", pkg), ("name", name)],
    )
}

// ---------------------------------------------------------------------------
// `generate durable-job` -- leased PostgreSQL work composed with a use case.
// ---------------------------------------------------------------------------

/// Generate at-least-once durable execution without teaching Jails a domain.
///
/// The work fields must exactly match an existing generated command and must
/// include its stable UUID `id`. `--yields` names the resource created by the
/// use case. That lets a reclaimed execution observe an already-committed
/// resource and mark the work successful after a crash between the business
/// commit and the queue acknowledgement.
pub(crate) fn durable_job_files(
    root: &Path,
    security: &str,
    jobs: &str,
    web: &str,
    service: &str,
    app: &str,
    domain: &str,
    name: &str,
    usecase: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    require_scope_authorizer(root, security, "durable-job", name, fields)?;
    let pom = std::fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if !crate::pom::has_dependency(&pom, "org.springframework.boot", "spring-boot-starter-jdbc") {
        return Err(format!(
            "durable-job {name} needs PostgreSQL/JDBC for durable leasing.\n       fix: run `jails add db` before generating it."
        ));
    }
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("durable-job {name} needs a stable `id:uuid` field"))?;
    if usecase_normalized_type(&id.java_type) != "UUID"
        || id.optionality == crate::generate::Optionality::Nullable
    {
        return Err(format!(
            "durable-job {name} needs required `id:uuid`; it received id:{}",
            id.java_type
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "durable-job {name} field `{}` is optional or a collection. Durable payload v1 accepts required scalar JDBC fields so storage and equality are exact.",
            field.name
        ));
    }
    let command_name = format!("{usecase}Command");
    let command_fields = crate::generate::fields_from_record(root, service, &command_name)
        .ok_or_else(|| {
            format!(
                "durable-job {name} cannot read {command_name}.java. Generate usecase {usecase} first."
            )
        })?;
    if fields.len() != command_fields.len()
        || fields.iter().zip(&command_fields).any(|(work, command)| {
            work.name != command.name
                || usecase_normalized_type(&work.java_type)
                    != usecase_normalized_type(&command.java_type)
                || (work.optionality == crate::generate::Optionality::Nullable)
                    != (command.optionality == crate::generate::Optionality::Nullable)
        })
    {
        let wanted = command_fields
            .iter()
            .map(|field| format!("{}:{}", field.name, usecase_field_type(field)))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "durable-job {name} fields must exactly match {command_name} in declaration order.\n       expected: {wanted}"
        ));
    }
    let target_fields = crate::generate::fields_from_record(root, domain, target)
        .ok_or_else(|| format!("durable-job {name} cannot read target resource {target}.java"))?;
    let target_id = target_fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("durable-job {name} target {target} has no stable id"))?;
    if usecase_normalized_type(&target_id.java_type) != "UUID" {
        return Err(format!(
            "durable-job {name} v1 needs {target}.id to be UUID so work and effect share one stable identity"
        ));
    }

    let columns = crate::sql::columns(fields, root, domain, "work");
    let unmapped = columns
        .iter()
        .filter(|column| !column.mapped())
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return Err(format!(
            "durable-job {name} cannot map payload column(s): {}",
            unmapped.join(", ")
        ));
    }

    let migration_dir = root.join("src/main/resources/db/migration");
    let version = crate::generate::next_migration_version(&migration_dir)?;
    let table = format!("{}_jobs", crate::sql::snake_case(name));
    let main_jobs = crate::generate::main_dir(root, jobs);
    let test_jobs = crate::generate::test_dir(root, jobs);
    let main_web = crate::generate::main_dir(root, web);
    Ok(vec![
        (
            main_jobs.join(format!("{name}Work.java")),
            durable_work_java(jobs, domain, name, fields),
            "durable work payload",
        ),
        (
            main_jobs.join(format!("{name}Queue.java")),
            durable_queue_java(jobs, name),
            "durable work queue port",
        ),
        (
            main_jobs.join(format!("Jdbc{name}Store.java")),
            durable_store_java(jobs, domain, name, &table, &columns),
            "durable PostgreSQL store",
        ),
        (
            main_jobs.join(format!("{name}Worker.java")),
            durable_worker_java(jobs, service, app, name, usecase, target, fields),
            "durable worker",
        ),
        (
            main_web.join(format!("{name}JobController.java")),
            durable_job_controller_java(security, jobs, web, name, fields),
            "durable job controller",
        ),
        (
            test_jobs.join(format!("{name}JobIT.java")),
            durable_job_it_java(root, jobs, app, domain, name, target, &table, fields),
            "durable job integration test",
        ),
        (
            migration_dir.join(format!("V{version:03}__create_{table}.sql")),
            durable_job_migration(&table, &columns),
            "durable job migration",
        ),
    ])
}

fn durable_work_java(
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let class = format!("{name}Work");
    let mut source = crate::generate::record_java(pkg, &class, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        source = source.replacen(&package, &format!("{package}\n{}\n", imports.join("\n")), 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {class} value."),
        &format!(" * Stable, persistable input for the {name} durable job."),
    )
}

fn durable_queue_java(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/** Application-facing durable work queue. Reusing an id requires equal payload. */
public interface {name}Queue {{

    void enqueue({name}Work work);

    Optional<Status> status(UUID id);

    enum State {{ PENDING, RUNNING, SUCCEEDED, FAILED }}

    record Status(UUID id, State state, int attempts, Instant nextAttemptAt,
                  Optional<String> lastError, Optional<Instant> completedAt) {{}}

    final class IdempotencyConflictException extends RuntimeException {{
        public IdempotencyConflictException(UUID id) {{
            super("work id " + id + " was already used with a different payload");
        }}
    }}
}}
"#
    )
}

fn durable_store_java(
    pkg: &str,
    domain: &str,
    name: &str,
    table: &str,
    columns: &[crate::sql::Column],
) -> String {
    let property = crate::sql::snake_case(name).replace('_', "-");
    let mut imports = crate::sql::imports(columns)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    for column in columns {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let names = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = columns
        .iter()
        .map(|column| format!(":{}", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let bindings = columns
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped durable column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let select = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let returning = columns
        .iter()
        .map(|column| format!("jobs.{} as {}", column.name, column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = columns
        .iter()
        .map(|column| format!("                    {}", column.read.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"package {pkg};

{imports}import java.sql.ResultSet;
import java.sql.SQLException;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** PostgreSQL queue with skip-locked claiming, leases, bounded retry and terminal failure. */
@Component
public class Jdbc{name}Store implements {name}Queue {{

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public Jdbc{name}Store(
            JdbcClient db,
            @Value("${{jobs.{property}.max-attempts:10}}") int maxAttempts,
            @Value("${{jobs.{property}.lease-seconds:30}}") int leaseSeconds) {{
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) {{
            throw new IllegalArgumentException("max attempts and lease seconds must be positive");
        }}
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }}

    @Override
    @Transactional
    public void enqueue({name}Work work) {{
        Objects.requireNonNull(work, "work is required");
        int inserted = db.sql("""
                        insert into {table} ({names}, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values ({placeholders}, 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
{bindings}
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0) {{
            var existing = findWork(work.id()).orElseThrow();
            if (!existing.equals(work)) {{
                throw new {name}Queue.IdempotencyConflictException(work.id());
            }}
        }}
    }}

    @Override
    public Optional<Status> status(UUID id) {{
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select id, state, attempts, next_attempt_at, last_error, completed_at
                        from {table}
                        where id = :id
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new Status(
                        rows.getObject("id", UUID.class),
                        State.valueOf(rows.getString("state")),
                        rows.getInt("attempts"),
                        rows.getObject("next_attempt_at", OffsetDateTime.class).toInstant(),
                        Optional.ofNullable(rows.getString("last_error")),
                        Optional.ofNullable(rows.getObject("completed_at", OffsetDateTime.class))
                                .map(OffsetDateTime::toInstant)))
                .optional();
    }}

    @Transactional
    public Optional<Claimed> claim() {{
        return db.sql("""
                        with candidate as (
                            select id
                            from {table}
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked
                            limit 1
                        )
                        update {table} jobs
                        set state = 'RUNNING',
                            attempts = jobs.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate
                        where jobs.id = candidate.id
                        returning {returning}, jobs.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query(Jdbc{name}Store::mapClaim)
                .optional();
    }}

    @Transactional
    public void succeed(UUID id) {{
        db.sql("""
                        update {table}
                        set state = 'SUCCEEDED', completed_at = now(), lease_until = null,
                            last_error = null
                        where id = :id and state = 'RUNNING'
                        """)
                .param("id", id)
                .update();
    }}

    @Transactional
    public void fail(UUID id, RuntimeException failure) {{
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        db.sql("""
                        update {table}
                        set state = case when attempts >= max_attempts then 'FAILED' else 'PENDING' end,
                            next_attempt_at = now() + make_interval(
                                    secs => least(300, cast(power(2, attempts) as integer))),
                            lease_until = null,
                            last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """)
                .param("id", id)
                .param("error", error)
                .update();
    }}

    private Optional<{name}Work> findWork(UUID id) {{
        return db.sql("select {select} from {table} where id = :id")
                .param("id", id)
                .query((rows, rowNumber) -> mapWork(rows))
                .optional();
    }}

    private static Claimed mapClaim(ResultSet rows, int rowNumber) throws SQLException {{
        return new Claimed(mapWork(rows), rows.getInt("attempts"));
    }}

    private static {name}Work mapWork(ResultSet rows) throws SQLException {{
        return new {name}Work(
{map_args});
    }}

    public record Claimed({name}Work work, int attempt) {{}}
}}
"#
    )
}

fn durable_worker_java(
    pkg: &str,
    service: &str,
    app: &str,
    name: &str,
    usecase: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let command_import = crate::generate::import_of(pkg, service, &format!("{usecase}Command"));
    let usecase_import = crate::generate::import_of(pkg, service, &format!("{usecase}UseCase"));
    let repo_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let args = fields
        .iter()
        .map(|field| format!("                    work.{}()", field.name))
        .collect::<Vec<_>>()
        .join(",\n");
    let property = crate::sql::snake_case(name).replace('_', "-");
    format!(
        r#"package {pkg};

{command_import}{usecase_import}{repo_import}import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** At-least-once worker; an expired lease is reclaimed after process death. */
@Component
public final class {name}Worker {{

    private static final Logger log = LoggerFactory.getLogger({name}Worker.class);
    private final Jdbc{name}Store store;
    private final {usecase}UseCase useCase;
    private final {target}Repository results;

    public {name}Worker(Jdbc{name}Store store, {usecase}UseCase useCase,
                       {target}Repository results) {{
        this.store = store;
        this.useCase = useCase;
        this.results = results;
    }}

    @Scheduled(
            fixedDelayString = "${{jobs.{property}.delay:PT1S}}",
            initialDelayString = "${{jobs.{property}.initial-delay:PT1S}}")
    public void run() {{
        try {{
            runOnce();
        }} catch (RuntimeException infrastructureFailure) {{
            log.error("{name} could not claim durable work; the schedule continues", infrastructureFailure);
        }}
    }}

    public void runOnce() {{
        store.claim().ifPresent(this::execute);
    }}

    private void execute(Jdbc{name}Store.Claimed claimed) {{
        var work = claimed.work();
        try {{
            // A process can die after the use-case transaction commits and
            // before this queue row is acknowledged. The stable shared id is
            // the recovery proof: do not repeat an already-visible effect.
            if (results.findById(String.valueOf(work.id())).isEmpty()) {{
                useCase.execute(new {usecase}Command(
{args}));
            }}
            store.succeed(work.id());
        }} catch (RuntimeException failure) {{
            store.fail(work.id(), failure);
            log.warn("{name} attempt {{}} failed", claimed.attempt(), failure);
        }}
    }}
}}
"#
    )
}

fn durable_job_controller_java(
    security: &str,
    jobs: &str,
    web: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let queue_import = crate::generate::import_of(web, jobs, &format!("{name}Queue"));
    let work_import = crate::generate::import_of(web, jobs, &format!("{name}Work"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "work");
    let path = format!("/jobs/{}", crate::sql::snake_case(name).replace('_', "-"));
    format!(
        r#"package {web};

{queue_import}{work_import}{scope_import}import jakarta.validation.Valid;
import java.net.URI;
import java.util.UUID;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

/** HTTP submission/status adapter for durable work. */
@RestController
@RequestMapping({name}JobController.PATH)
public final class {name}JobController {{

    public static final String PATH = "{path}";
    private final {name}Queue queue;
{scope_field}

    public {name}JobController({name}Queue queue{scope_constructor}) {{
        this.queue = queue;
{scope_assignment}
    }}

    @PostMapping
    public ResponseEntity<{name}Queue.Status> enqueue(
            @Valid @RequestBody {name}Work work{scope_parameter}) {{
{scope_checks}
        try {{
            queue.enqueue(work);
        }} catch ({name}Queue.IdempotencyConflictException conflict) {{
            throw new ResponseStatusException(CONFLICT, conflict.getMessage(), conflict);
        }}
        var status = queue.status(work.id()).orElseThrow();
        return ResponseEntity.accepted()
                .location(URI.create(PATH + "/" + work.id()))
                .body(status);
    }}

    @GetMapping("/{{id}}")
    public {name}Queue.Status status(@PathVariable UUID id) {{
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(NOT_FOUND, "work not found"));
    }}
}}
"#
    )
}

fn durable_job_it_java(
    root: &Path,
    pkg: &str,
    app: &str,
    domain: &str,
    name: &str,
    target: &str,
    table: &str,
    fields: &[crate::generate::Field],
) -> String {
    let property = crate::sql::snake_case(name).replace('_', "-");
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = samples.is_none();
    let args = samples.unwrap_or_default().join(",\n                ");
    let alternate = fields.iter().enumerate().find_map(|(index, field)| {
        (field.name != "id")
            .then(|| durable_alternate_sample(field))
            .flatten()
            .map(|value| (index, value))
    });
    let conflict_test = alternate.map_or_else(String::new, |(changed, alternate)| {
        let alternate_args = fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                if index == changed {
                    alternate.clone()
                } else {
                    crate::generate::sample_value(field, root, domain).unwrap()
                }
            })
            .collect::<Vec<_>>()
            .join(",\n                ");
        format!(
            r#"
    @Test
    void reusingAnIdWithDifferentPayloadIsAConflict() {{
        var original = new {name}Work(
                {args});
        var conflicting = new {name}Work(
                {alternate_args});

        queue.enqueue(original);

        assertThatThrownBy(() -> queue.enqueue(conflicting))
                .isInstanceOf({name}Queue.IdempotencyConflictException.class);
    }}
"#
        )
    });
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply a durable-work sample Jails cannot fabricate\")\n"
    } else {
        ""
    };
    format!(
        r#"package {pkg};

{repository_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

{annotation}@SpringBootTest(properties = {{
        "jobs.{property}.initial-delay=PT1H",
        "jobs.{property}.max-attempts=2"
}})
@org.springframework.transaction.annotation.Transactional
class {name}JobIT {{

    @Autowired private {name}Queue queue;
    @Autowired private {name}Worker worker;
    @Autowired private Jdbc{name}Store store;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;
    @Autowired private {target}Repository results;

    @Test
    void committedWorkRunsAndRepeatingTheSameIdIsIdempotent() {{
        var work = new {name}Work(
                {args});

        queue.enqueue(work);
        queue.enqueue(work);
        worker.runOnce();

        assertThat(results.findById(String.valueOf(work.id()))).isPresent();
        assertThat(queue.status(work.id())).get()
                .extracting({name}Queue.Status::state)
                .isEqualTo({name}Queue.State.SUCCEEDED);
    }}

    @Test
    void anExpiredLeaseIsReclaimedAndBoundedFailureBecomesVisible() {{
        var work = new {name}Work(
                {args});
        queue.enqueue(work);

        assertThat(store.claim()).isPresent();
        db.sql("update {table} set lease_until = now() - interval '1 second' where id = :id")
                .param("id", work.id())
                .update();
        var reclaimed = store.claim().orElseThrow();
        store.fail(work.id(), new IllegalStateException("test failure"));

        assertThat(reclaimed.attempt()).isEqualTo(2);
        assertThat(queue.status(work.id())).get()
                .satisfies(status -> {{
                    assertThat(status.state()).isEqualTo({name}Queue.State.FAILED);
                    assertThat(status.lastError()).contains("test failure");
                }});
    }}
{conflict_test}
}}
"#
    )
}

fn durable_alternate_sample(field: &crate::generate::Field) -> Option<String> {
    match usecase_normalized_type(&field.java_type) {
        "String" => Some("\"different-payload\"".to_string()),
        "UUID" => Some("UUID.fromString(\"00000000-0000-0000-0000-000000000002\")".to_string()),
        "URI" => Some("URI.create(\"https://different.example.test/\")".to_string()),
        "Integer" => Some("2".to_string()),
        "Long" => Some("2L".to_string()),
        "Double" => Some("2.5".to_string()),
        "Boolean" => Some("false".to_string()),
        _ => None,
    }
}

fn durable_job_migration(table: &str, columns: &[crate::sql::Column]) -> String {
    let payload = columns
        .iter()
        .map(|column| format!("  {} {} not null,", column.name, column.sql_type))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "-- Durable, leased, at-least-once work.\n\
         create table {table} (\n\
         {payload}\n\
           state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),\n\
           attempts integer not null check (attempts >= 0),\n\
           max_attempts integer not null check (max_attempts > 0),\n\
           next_attempt_at timestamptz not null,\n\
           lease_until timestamptz,\n\
           last_error text,\n\
           created_at timestamptz not null,\n\
           completed_at timestamptz,\n\
           constraint {table}_pk primary key (id)\n\
         );\n\n\
         create index {table}_runnable_idx\n\
           on {table} (state, next_attempt_at)\n\
           where state in ('PENDING', 'RUNNING');\n"
    )
}

/// Attach a generated use case to a typed event through a transactional
/// PostgreSQL outbox. `usecase --yields Event` is deliberately composition,
/// not a second domain-specific workflow language: the event's components
/// must come from the command/result or one safe timestamp default.
pub(crate) fn outbox_files(
    root: &Path,
    service: &str,
    domain: &str,
    app: &str,
    adapters: &str,
    messaging: &str,
    jobs: &str,
    usecase: &str,
    target: &str,
    event: &str,
    command_fields: &[crate::generate::Field],
) -> crate::Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    let json = crate::generate::main_dir(root, adapters).join("Json.java");
    if !json.exists() {
        return Err(format!(
            "usecase {usecase} --yields {event} needs the generic JSON capability for durable payloads.\n       fix: run `jails add json` first."
        ));
    }
    let event_class = format!("{event}Event");
    let event_fields = crate::generate::fields_from_record(root, messaging, &event_class)
        .ok_or_else(|| {
            format!(
                "usecase {usecase} yields {event}, but {event_class}.java does not exist or is not a record. Generate the typed event first."
            )
        })?;
    let event_id = event_fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("outbox event {event_class} needs a stable id"))?;
    if usecase_normalized_type(&event_id.java_type) != "UUID"
        || event_id.optionality == crate::generate::Optionality::Nullable
    {
        return Err(format!(
            "transactional outbox v1 requires {event_class}.id to be a required UUID"
        ));
    }
    let target_fields = crate::generate::fields_from_record(root, domain, target)
        .ok_or_else(|| format!("usecase {usecase} cannot read target {target}.java"))?;
    let mut expressions = Vec::with_capacity(event_fields.len());
    let mut needs_instant = false;
    for event_field in &event_fields {
        if let Some(field) = target_fields
            .iter()
            .find(|candidate| candidate.name == event_field.name)
        {
            ensure_outbox_type(usecase, event_field, field, target)?;
            expressions.push(format!("result.{}()", field.name));
        } else if let Some(field) = command_fields
            .iter()
            .find(|candidate| candidate.name == event_field.name)
        {
            ensure_outbox_type(usecase, event_field, field, "command")?;
            expressions.push(format!("command.{}()", field.name));
        } else if event_field.java_type == "Instant"
            && event_field.optionality != crate::generate::Optionality::Nullable
            && event_field.name.ends_with("At")
        {
            needs_instant = true;
            expressions.push("Instant.now()".to_string());
        } else {
            return Err(format!(
                "usecase {usecase} cannot derive event field `{}` for {event_class}.\n       fix: use a component from the command/result, or a required Instant name ending in `At`.",
                event_field.name
            ));
        }
    }
    let table = format!("{}_outbox", crate::sql::snake_case(usecase));
    let property = crate::sql::snake_case(usecase).replace('_', "-");
    let migration_dir = root.join("src/main/resources/db/migration");
    let version = crate::generate::next_migration_version(&migration_dir)?;
    let main_service = crate::generate::main_dir(root, service);
    let main_jobs = crate::generate::main_dir(root, jobs);
    let test_jobs = crate::generate::test_dir(root, jobs);
    Ok(vec![
        (
            main_service.join(format!("Outbox{usecase}UseCase.java")),
            outbox_usecase_java(
                service,
                domain,
                messaging,
                jobs,
                usecase,
                target,
                event,
                &expressions,
                needs_instant,
            ),
            "transactional outbox use case",
        ),
        (
            main_jobs.join(format!("Jdbc{usecase}Outbox.java")),
            outbox_store_java(jobs, adapters, messaging, usecase, event, &table, &property),
            "transactional outbox store",
        ),
        (
            main_jobs.join(format!("{usecase}OutboxWorker.java")),
            outbox_worker_java(jobs, messaging, usecase, event, &property),
            "transactional outbox worker",
        ),
        (
            test_jobs.join(format!("{usecase}OutboxIT.java")),
            outbox_it_java(
                root,
                jobs,
                service,
                domain,
                app,
                usecase,
                target,
                &property,
                command_fields,
            ),
            "transactional outbox integration test",
        ),
        (
            migration_dir.join(format!("V{version:03}__create_{table}.sql")),
            outbox_migration(&table),
            "transactional outbox migration",
        ),
    ])
}

fn ensure_outbox_type(
    usecase: &str,
    event: &crate::generate::Field,
    source: &crate::generate::Field,
    owner: &str,
) -> crate::Result<()> {
    if usecase_normalized_type(&event.java_type) != usecase_normalized_type(&source.java_type)
        || (event.optionality == crate::generate::Optionality::Nullable)
            != (source.optionality == crate::generate::Optionality::Nullable)
    {
        return Err(format!(
            "usecase {usecase} cannot map event field `{}` ({}) from {owner} ({})",
            event.name, event.java_type, source.java_type
        ));
    }
    Ok(())
}

fn outbox_usecase_java(
    service: &str,
    domain: &str,
    messaging: &str,
    jobs: &str,
    usecase: &str,
    target: &str,
    event: &str,
    expressions: &[String],
    needs_instant: bool,
) -> String {
    let target_import = crate::generate::import_of(service, domain, target);
    let event_import = crate::generate::import_of(service, messaging, &format!("{event}Event"));
    let store_import = crate::generate::import_of(service, jobs, &format!("Jdbc{usecase}Outbox"));
    let instant_import = if needs_instant {
        "import java.time.Instant;\n"
    } else {
        ""
    };
    let args = expressions
        .iter()
        .map(|expression| format!("                {expression}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"package {service};

{target_import}{event_import}{store_import}{instant_import}import java.util.Objects;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** Creates the resource and stages its event in the same database transaction. */
@Primary
@Component
public class Outbox{usecase}UseCase implements {usecase}UseCase {{

    private final Default{usecase}UseCase delegate;
    private final Jdbc{usecase}Outbox outbox;

    public Outbox{usecase}UseCase(Default{usecase}UseCase delegate, Jdbc{usecase}Outbox outbox) {{
        this.delegate = Objects.requireNonNull(delegate, "delegate is required");
        this.outbox = Objects.requireNonNull(outbox, "outbox is required");
    }}

    @Override
    @Transactional
    public {target} execute({usecase}Command command) {{
        var result = delegate.execute(command);
        outbox.stage(new {event}Event(
{args}));
        return result;
    }}
}}
"#
    )
}

fn outbox_store_java(
    pkg: &str,
    adapters: &str,
    messaging: &str,
    usecase: &str,
    event: &str,
    table: &str,
    property: &str,
) -> String {
    let json_import = crate::generate::import_of(pkg, adapters, "Json");
    let event_import = crate::generate::import_of(pkg, messaging, &format!("{event}Event"));
    format!(
        r#"package {pkg};

{json_import}{event_import}import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** PostgreSQL transactional outbox with leases, bounded retry and stable event identity. */
@Component
public class Jdbc{usecase}Outbox {{

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public Jdbc{usecase}Outbox(
            JdbcClient db,
            @Value("${{outbox.{property}.max-attempts:10}}") int maxAttempts,
            @Value("${{outbox.{property}.lease-seconds:30}}") int leaseSeconds) {{
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) throw new IllegalArgumentException("positive limits required");
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }}

    @Transactional
    public void stage({event}Event event) {{
        Objects.requireNonNull(event, "event is required");
        String payload = Json.toJson(event);
        int inserted = db.sql("""
                        insert into {table} (id, payload, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values (:id, cast(:payload as jsonb), 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
                .param("id", event.id())
                .param("payload", payload)
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0) {{
            var existing = db.sql("select payload::text from {table} where id = :id")
                    .param("id", event.id()).query(String.class).single();
            if (!Json.parse(existing, {event}Event.class).equals(event)) {{
                throw new IllegalStateException("event id already staged with different payload: " + event.id());
            }}
        }}
    }}

    public Optional<Status> status(UUID id) {{
        return db.sql("""
                        select id, state, attempts, last_error, completed_at
                        from {table} where id = :id
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new Status(
                        rows.getObject("id", UUID.class),
                        State.valueOf(rows.getString("state")), rows.getInt("attempts"),
                        Optional.ofNullable(rows.getString("last_error")),
                        Optional.ofNullable(rows.getObject("completed_at", OffsetDateTime.class))
                                .map(OffsetDateTime::toInstant)))
                .optional();
    }}

    @Transactional
    public Optional<Claimed> claim() {{
        return db.sql("""
                        with candidate as (
                            select id from {table}
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked limit 1
                        )
                        update {table} events
                        set state = 'RUNNING', attempts = events.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate where events.id = candidate.id
                        returning events.id, events.payload::text as payload, events.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query((rows, rowNumber) -> new Claimed(
                        rows.getObject("id", UUID.class),
                        Json.parse(rows.getString("payload"), {event}Event.class),
                        rows.getInt("attempts")))
                .optional();
    }}

    @Transactional
    public void succeed(UUID id) {{
        db.sql("""
                        update {table} set state = 'SUCCEEDED', lease_until = null,
                            last_error = null, completed_at = now()
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).update();
    }}

    @Transactional
    public void fail(UUID id, RuntimeException failure) {{
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        db.sql("""
                        update {table}
                        set state = case when attempts >= max_attempts then 'FAILED' else 'PENDING' end,
                            next_attempt_at = now() + make_interval(
                                    secs => least(300, cast(power(2, attempts) as integer))),
                            lease_until = null, last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).param("error", error).update();
    }}

    public enum State {{ PENDING, RUNNING, SUCCEEDED, FAILED }}
    public record Status(UUID id, State state, int attempts,
                         Optional<String> lastError, Optional<Instant> completedAt) {{}}
    public record Claimed(UUID id, {event}Event event, int attempt) {{}}
}}
"#
    )
}

fn outbox_worker_java(
    pkg: &str,
    messaging: &str,
    usecase: &str,
    event: &str,
    property: &str,
) -> String {
    let publisher_import = crate::generate::import_of(pkg, messaging, &format!("{event}Publisher"));
    format!(
        r#"package {pkg};

{publisher_import}import java.util.concurrent.CompletionException;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** Leased outbox relay; success means Kafka acknowledged the record, not merely accepted a future. */
@Component
public final class {usecase}OutboxWorker {{

    private static final Logger log = LoggerFactory.getLogger({usecase}OutboxWorker.class);
    private final Jdbc{usecase}Outbox outbox;
    private final {event}Publisher publisher;

    public {usecase}OutboxWorker(Jdbc{usecase}Outbox outbox, {event}Publisher publisher) {{
        this.outbox = outbox;
        this.publisher = publisher;
    }}

    @Scheduled(
            fixedDelayString = "${{outbox.{property}.delay:PT1S}}",
            initialDelayString = "${{outbox.{property}.initial-delay:PT1S}}")
    public void run() {{
        try {{ runOnce(); }}
        catch (RuntimeException infrastructureFailure) {{
            log.error("{usecase} outbox could not claim work; the schedule continues", infrastructureFailure);
        }}
    }}

    public void runOnce() {{ outbox.claim().ifPresent(this::publish); }}

    private void publish(Jdbc{usecase}Outbox.Claimed claimed) {{
        try {{
            publisher.publish(claimed.event()).join();
            outbox.succeed(claimed.id());
        }} catch (CompletionException failure) {{
            var cause = failure.getCause();
            var recorded = cause instanceof RuntimeException runtime ? runtime : failure;
            outbox.fail(claimed.id(), recorded);
            log.warn("{usecase} outbox attempt {{}} failed", claimed.attempt(), recorded);
        }} catch (RuntimeException failure) {{
            outbox.fail(claimed.id(), failure);
            log.warn("{usecase} outbox attempt {{}} failed", claimed.attempt(), failure);
        }}
    }}
}}
"#
    )
}

fn outbox_it_java(
    root: &Path,
    pkg: &str,
    service: &str,
    domain: &str,
    app: &str,
    usecase: &str,
    target: &str,
    property: &str,
    fields: &[crate::generate::Field],
) -> String {
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = samples.is_none();
    let args = samples.unwrap_or_default().join(",\n                ");
    let command_import = crate::generate::import_of(pkg, service, &format!("{usecase}Command"));
    let usecase_import = crate::generate::import_of(pkg, service, &format!("{usecase}UseCase"));
    let target_import = crate::generate::import_of(pkg, domain, target);
    let repo_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply outbox command samples\")\n"
    } else {
        ""
    };
    format!(
        r#"package {pkg};

{command_import}{usecase_import}{target_import}{repo_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;

{annotation}@SpringBootTest(properties = {{
        "outbox.{property}.initial-delay=PT1H",
        "outbox.{property}.max-attempts=2"
}})
@org.springframework.transaction.annotation.Transactional
class {usecase}OutboxIT {{

    @Autowired private {usecase}UseCase useCase;
    @Autowired private {target}Repository results;
    @Autowired private Jdbc{usecase}Outbox outbox;
    @Autowired private {usecase}OutboxWorker worker;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;

    @Test
    void businessEffectAndEventAreStagedTogetherThenKafkaAcknowledgementCompletesDelivery() {{
        var command = new {usecase}Command(
                {args});

        var result = useCase.execute(command);

        assertThat(results.findById(String.valueOf(result.id())))
                .get().extracting({target}::id).isEqualTo(result.id());
        assertThat(outbox.status(result.id())).get()
                .extracting(Jdbc{usecase}Outbox.Status::state)
                .isEqualTo(Jdbc{usecase}Outbox.State.PENDING);

        worker.runOnce();

        assertThat(outbox.status(result.id())).get()
                .extracting(Jdbc{usecase}Outbox.Status::state)
                .isEqualTo(Jdbc{usecase}Outbox.State.SUCCEEDED);
    }}

    @Test
    void retriesKeepTheStableEventIdAndTerminalFailureIsInspectable() {{
        var command = new {usecase}Command(
                {args});
        var result = useCase.execute(command);

        var first = outbox.claim().orElseThrow();
        outbox.fail(first.id(), new IllegalStateException("provider unavailable"));
        db.sql("update {usecase_snake}_outbox set next_attempt_at = now() where id = :id")
                .param("id", first.id()).update();
        var second = outbox.claim().orElseThrow();
        outbox.fail(second.id(), new IllegalStateException("provider unavailable"));

        assertThat(second.id()).isEqualTo(result.id()).isEqualTo(first.id());
        assertThat(outbox.status(result.id())).get().satisfies(status -> {{
            assertThat(status.state()).isEqualTo(Jdbc{usecase}Outbox.State.FAILED);
            assertThat(status.lastError()).contains("provider unavailable");
        }});
    }}
}}
"#,
        usecase_snake = crate::sql::snake_case(usecase)
    )
}

fn outbox_migration(table: &str) -> String {
    format!(
        "-- Transactional outbox: business writes and event staging share one commit.\n\
         create table {table} (\n\
           id uuid primary key,\n\
           payload jsonb not null,\n\
           state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),\n\
           attempts integer not null check (attempts >= 0),\n\
           max_attempts integer not null check (max_attempts > 0),\n\
           next_attempt_at timestamptz not null,\n\
           lease_until timestamptz,\n\
           last_error text,\n\
           created_at timestamptz not null,\n\
           completed_at timestamptz\n\
         );\n\n\
         create index {table}_runnable_idx on {table} (state, next_attempt_at)\n\
           where state in ('PENDING', 'RUNNING');\n"
    )
}

#[cfg(test)]
mod durable_job_tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-durable-job-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for package in ["domain", "service"] {
            std::fs::create_dir_all(root.join(format!("src/main/java/com/example/demo/{package}")))
                .unwrap();
        }
        std::fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            "<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency></dependencies></project>",
        )
        .unwrap();
        root
    }

    fn write_record(root: &Path, package: &str, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!(
                "src/main/java/com/example/demo/{package}/{name}.java"
            )),
            crate::generate::record_java(&format!("com.example.demo.{package}"), name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn durable_job_has_leasing_bounded_retry_idempotency_and_recovery() {
        let root = fixture("contract");
        write_record(&root, "domain", "CrawlRun", &["id:uuid", "seedUrl:uri"]);
        write_record(
            &root,
            "service",
            "QueueCrawlCommand",
            &["id:uuid", "seedUrl:uri"],
        );
        let fields =
            crate::generate::parse_fields(&["id:uuid".to_string(), "seedUrl:uri".to_string()])
                .unwrap();

        let files = durable_job_files(
            &root,
            "com.example.demo",
            "com.example.demo.jobs",
            "com.example.demo.web",
            "com.example.demo.service",
            "com.example.demo.app",
            "com.example.demo.domain",
            "CrawlDispatcher",
            "QueueCrawl",
            "CrawlRun",
            &fields,
        )
        .unwrap();
        let store = &files
            .iter()
            .find(|(_, _, kind)| *kind == "durable PostgreSQL store")
            .unwrap()
            .1;
        let worker = &files
            .iter()
            .find(|(_, _, kind)| *kind == "durable worker")
            .unwrap()
            .1;
        let migration = &files
            .iter()
            .find(|(_, _, kind)| *kind == "durable job migration")
            .unwrap()
            .1;

        assert!(store.contains("for update skip locked"), "{store}");
        assert!(store.contains("lease_until <= now()"), "{store}");
        assert!(store.contains("attempts >= max_attempts"), "{store}");
        assert!(store.contains("on conflict (id) do nothing"), "{store}");
        assert!(store.contains("jobs.id as id"), "{store}");
        assert!(worker.contains("results.findById"), "{worker}");
        assert!(worker.contains("store.succeed(work.id())"), "{worker}");
        assert!(migration.contains("state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')"));
    }

    #[test]
    fn durable_job_requires_a_stable_id_shared_with_the_command() {
        let root = fixture("identity");
        write_record(&root, "domain", "CrawlRun", &["id:uuid", "seedUrl:uri"]);
        write_record(&root, "service", "QueueCrawlCommand", &["seedUrl:uri"]);
        let fields = crate::generate::parse_fields(&["seedUrl:uri".to_string()]).unwrap();

        let error = durable_job_files(
            &root,
            "com.example.demo",
            "com.example.demo.jobs",
            "com.example.demo.web",
            "com.example.demo.service",
            "com.example.demo.app",
            "com.example.demo.domain",
            "CrawlDispatcher",
            "QueueCrawl",
            "CrawlRun",
            &fields,
        )
        .unwrap_err();

        assert!(error.contains("stable `id:uuid`"), "{error}");
    }
}

// ---------------------------------------------------------------------------
// `generate dto` -- the request and response shapes for a domain type.
// ---------------------------------------------------------------------------

/// Request/response records for a domain type, plus the mapping between them.
///
/// This is the most-typed, least-thought-about code in a Spring service, and
/// skipping it is worse than writing it: exposing a domain record directly as
/// the API contract means every internal rename is a breaking change for
/// clients, and every new field is published whether or not anyone meant to.
///
/// The request carries bean-validation annotations derived from the field
/// spec jails already has -- a non-null component becomes `@NotNull`, a
/// non-blank one `@NotBlank` -- so a malformed request is rejected at the
/// edge and reported by `add api`'s handler as a 400 naming the field.
pub(crate) fn dto_files(
    root: &Path,
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let domain_import = crate::generate::import_of(pkg, domain, name);
    vec![
        (
            main.join(format!("{name}Request.java")),
            request_java_for(pkg, name, fields, &domain_import, domain),
            "request",
        ),
        (
            main.join(format!("{name}Response.java")),
            response_java_for(pkg, name, fields, &domain_import, domain),
            "response",
        ),
        (
            test.join(format!("{name}DtoTest.java")),
            dto_test_java(pkg, name, fields, &domain_import, root, domain),
            "dto test",
        ),
    ]
}

/// Which validation annotation a component earns, from the optionality jails
/// already parsed. Returns the annotation and the import it needs.
fn validation_for(field: &crate::generate::Field) -> Option<(&'static str, &'static str)> {
    use crate::generate::Optionality;
    // A primitive cannot be null, so @NotNull on one is noise at best -- and
    // Hibernate Validator rejects some constraint/type pairings outright.
    if is_primitive(&field.java_type) {
        return None;
    }
    match field.optionality {
        // `!` means non-blank, which only applies to text -- and @NotBlank
        // implies @NotNull, so one annotation covers both.
        Optionality::NonBlank => Some(("@NotBlank", "jakarta.validation.constraints.NotBlank")),
        Optionality::Required => Some(("@NotNull", "jakarta.validation.constraints.NotNull")),
        // `?` is explicitly optional: constraining it would contradict the
        // field spec.
        Optionality::Nullable => None,
    }
}

fn is_primitive(java_type: &str) -> bool {
    matches!(
        java_type,
        "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short"
    )
}

/// The DTO's own component type. An `Optional<T>` domain component becomes a
/// plain nullable `T` on the wire: JSON has `null` and no notion of an
/// absent-vs-null-valued Optional, and Jackson serialising an `Optional`
/// without the JDK8 module produces `{"present":true}` rather than the value.
fn wire_type(field: &crate::generate::Field) -> String {
    // `java_type` is always the inner type; `optionality` says whether the
    // record wraps it. The wire type is the inner one either way.
    field.java_type.clone()
}

/// Imports for the DTO's own components.
///
/// `owner`/`user` are the domain package and the DTO's package: a component
/// whose type the project declares (an enum, most often) needs importing from
/// wherever the domain lives, and `field.imports` cannot carry that because
/// jails only knows the built-in types' packages. Missing it produces a
/// record that names a type it cannot see, which javac catches and no
/// template review does.
fn dto_imports(
    fields: &[crate::generate::Field],
    with_validation: bool,
    owner: &str,
    user: &str,
) -> String {
    let mut imports: Vec<String> = Vec::new();
    for field in fields {
        if field.owned {
            let import = crate::generate::import_of(user, owner, &field.java_type);
            if !import.is_empty() {
                imports.push(
                    import
                        .trim()
                        .trim_start_matches("import ")
                        .trim_end_matches(';')
                        .to_string(),
                );
            }
        }
        for import in &field.imports {
            // Optional itself never reaches the wire type, so its import
            // would be unused -- and an unused import fails `jails check`
            // under a strict formatter.
            if *import == "java.util.Optional" {
                continue;
            }
            imports.push((*import).to_string());
        }
        if with_validation && let Some((_, import)) = validation_for(field) {
            imports.push(import.to_string());
        }
    }
    imports.sort();
    imports.dedup();
    imports
        .into_iter()
        .map(|i| format!("import {i};\n"))
        .collect()
}

fn components(fields: &[crate::generate::Field], with_validation: bool) -> String {
    fields
        .iter()
        .map(|field| {
            let annotation = if with_validation {
                validation_for(field)
                    .map(|(a, _)| format!("{a} "))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!("        {annotation}{} {}", wire_type(field), field.name)
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

/// `x.name()` for a plain component, `x.name().orElse(null)` for an Optional
/// one -- the wire type is nullable, so the Optional is unwrapped exactly
/// once, here, rather than at every call site.
fn read_from_domain(field: &crate::generate::Field, receiver: &str) -> String {
    let accessor = format!("{receiver}.{}()", field.name);
    if is_optional(field) {
        format!("{accessor}.orElse(null)")
    } else {
        accessor
    }
}

fn is_optional(field: &crate::generate::Field) -> bool {
    field.optionality == crate::generate::Optionality::Nullable
}

/// The reverse: a nullable wire component becomes an `Optional` again. The
/// generated record's compact constructor normalises a null Optional, so
/// `ofNullable` is enough.
fn write_to_domain(field: &crate::generate::Field) -> String {
    if is_optional(field) {
        format!("Optional.ofNullable({})", field.name)
    } else {
        field.name.clone()
    }
}

fn needs_optional(fields: &[crate::generate::Field]) -> bool {
    fields.iter().any(is_optional)
}

pub(crate) fn request_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, true, domain, pkg);
    let optional_import = if needs_optional(fields) {
        "import java.util.Optional;\n"
    } else {
        ""
    };
    let components = components(fields, true);
    let arguments = fields
        .iter()
        .map(write_to_domain)
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"package {pkg};

{domain_import}{optional_import}{imports}
/**
 * What a client may send. Deliberately not {name} itself.
 *
 * <p>A domain type used as the wire contract couples the two permanently:
 * renaming a component becomes a breaking API change, and adding one
 * publishes it whether or not that was intended. The cost of keeping them
 * apart is this file; the cost of not doing is paid later and by someone else.
 *
 * <p>The constraints come from the field spec, so a malformed request is
 * rejected before any application code runs. With {{@code jails add api}} the
 * rejection is reported as a 400 naming each bad field.
 */
public record {name}Request(
{components}) {{

    /** @return the domain type this request describes. */
    public {name} toDomain() {{
        return new {name}(
{arguments});
    }}
}}
"#
    )
}

pub(crate) fn response_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, false, domain, pkg);
    let components = components(fields, false);
    let arguments = fields
        .iter()
        .map(|field| read_from_domain(field, &crate::generate::lower_first(name)))
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = crate::generate::lower_first(name);
    format!(
        r#"package {pkg};

{domain_import}{imports}
/**
 * What this application returns. Deliberately not {name} itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{{@code from}} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record {name}Response(
{components}) {{

    /** @return the response describing {{@code {var}}}. */
    public static {name}Response from({name} {var}) {{
        return new {name}Response(
{arguments});
    }}
}}
"#
    )
}

/// The round-trip test.
///
/// jails follows one rule for a test it cannot fully write: emit it whole and
/// `@Disabled`, naming what is missing. Emitting a guess would produce a test
/// that does not compile; emitting nothing would drop the coverage silently.
/// Here the guess would be a sample value for a component whose type jails
/// has no model of, so the sample is attempted per component and the whole
/// test is disabled only if some component defeats it.
fn dto_test_java(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    root: &Path,
    domain: &str,
) -> String {
    let var = crate::generate::lower_first(name);
    // A request component is the *wire* type: an Optional domain component is
    // a plain nullable field here, so `Optional.empty()` would not compile as
    // its sample. `null` is the honest wire-level equivalent.
    let samples: Vec<Option<String>> = fields
        .iter()
        .map(|field| {
            if is_optional(field) {
                Some("null".to_string())
            } else {
                crate::generate::sample_value(field, root, domain)
            }
        })
        .collect();
    let unsampleable: Vec<&str> = fields
        .iter()
        .zip(&samples)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.java_type.as_str())
        .collect();

    let disabled = if unsampleable.is_empty() {
        String::new()
    } else {
        format!(
            "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
            unsampleable.join(", ")
        )
    };
    let disabled_import = if unsampleable.is_empty() {
        String::new()
    } else {
        "import org.junit.jupiter.api.Disabled;\n".to_string()
    };
    let arguments = fields
        .iter()
        .zip(&samples)
        .map(|(field, sample)| {
            format!(
                "                {}",
                sample
                    .clone()
                    .unwrap_or_else(|| format!("null /* {} */", field.java_type))
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    // The sample literals need the same imports the wire types do
    // (`UUID.fromString`, `Instant.parse`, ...), and `dto_imports` already
    // computes exactly that set with Optional filtered out.
    let sample_imports = dto_imports(fields, false, domain, pkg);

    format!(
        r#"package {pkg};

{domain_import}{sample_imports}{disabled_import}import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * The round trip is the property worth pinning: whatever a request describes
 * must survive being turned into the domain type and back into a response.
 *
 * <p>Two records that drift apart still compile -- a component added to one
 * and not the other is silently dropped on the wire -- so this is the test
 * that notices.
 */
{disabled}class {name}DtoTest {{

    @Test
    void aRequestSurvivesTheRoundTripToAResponse() {{
        {name}Request request = sample();
        {name} {var} = request.toDomain();
        {name}Response response = {name}Response.from({var});

        assertThat(response).isNotNull();
        // Every component exists on both records -- the compiler has already
        // checked that much. What to assert here is which ones matter.
    }}

    private static {name}Request sample() {{
        return new {name}Request(
{arguments});
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// `generate usecase` -- an executable create operation over a scaffold.
// ---------------------------------------------------------------------------

pub(crate) fn require_scope_authorizer(
    root: &Path,
    security: &str,
    kind: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<()> {
    if !fields.iter().any(|field| field.constraints.scoped) {
        return Ok(());
    }
    let guard = crate::generate::main_dir(root, security).join("ScopeAuthorizer.java");
    if !guard.exists() {
        return Err(format!(
            "{kind} {name} uses @scope, but the project has no ScopeAuthorizer.\n       fix: run `jails add security` before generating scoped HTTP operations."
        ));
    }
    Ok(())
}

fn scope_controller_parts(
    security: &str,
    web: &str,
    fields: &[crate::generate::Field],
    request: &str,
) -> (String, String, String, String, String, String) {
    let scoped = fields
        .iter()
        .filter(|field| field.constraints.scoped)
        .collect::<Vec<_>>();
    if scoped.is_empty() {
        return (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
    }
    let import = format!(
        "{}import org.springframework.security.core.Authentication;\n",
        crate::generate::import_of(web, security, "ScopeAuthorizer")
    );
    let checks = scoped
        .iter()
        .map(|field| {
            format!(
                "        scopeAuthorizer.require(authentication, \"{}\", {request}.{}());",
                field.name, field.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    (
        import,
        "    private final ScopeAuthorizer scopeAuthorizer;\n".to_string(),
        ", ScopeAuthorizer scopeAuthorizer".to_string(),
        "        this.scopeAuthorizer = scopeAuthorizer;".to_string(),
        ",\n            Authentication authentication".to_string(),
        checks,
    )
}

fn scope_test_parts(
    security: &str,
    web: &str,
    fields: &[crate::generate::Field],
) -> (String, String) {
    if !fields.iter().any(|field| field.constraints.scoped) {
        return (String::new(), String::new());
    }
    (
        format!(
            "{}import org.springframework.mock.env.MockEnvironment;\n",
            crate::generate::import_of(web, security, "ScopeAuthorizer")
        ),
        r#"
        @Bean
        ScopeAuthorizer scopeAuthorizer() {
            return new ScopeAuthorizer(new MockEnvironment());
        }
"#
        .to_string(),
    )
}

/// Turn a small operation declaration into a complete vertical behavior.
///
/// `fields` are the values a caller supplies; `target` is an existing
/// scaffolded record named by `--on`. Every target component must either be
/// supplied or have one conservative conventional value Jails can prove how
/// to construct (identity, timestamp, empty optional/collection, zero counter,
/// false flag, or the first declared `status` enum constant). Anything else
/// is rejected at generation time rather than becoming a TODO in production
/// code.
pub(crate) fn usecase_files(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    app: &str,
    adapters: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    require_scope_authorizer(root, security, "usecase", name, fields)?;
    let target_fields = crate::generate::fields_from_record(root, domain, target).ok_or_else(|| {
        format!(
            "usecase {name} targets {target}, but no record components could be read from {target}.java.\n       fix: generate the {target} scaffold first, or correct `--on {target}`."
        )
    })?;
    let id = target_fields
        .iter()
        .find(|field| {
            field.name == "id"
                && field.optionality != crate::generate::Optionality::Nullable
        })
        .ok_or_else(|| {
            format!(
                "usecase {name} needs {target} to have a stable non-optional `id` component so it can return a resource location and verify persistence"
            )
        })?;

    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "usecase {name} accepts `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
            || (field.optionality == crate::generate::Optionality::Nullable)
                != (target_field.optionality == crate::generate::Optionality::Nullable)
        {
            return Err(format!(
                "usecase {name} declares `{}` as {}, but {target} declares it as {}{}",
                field.name,
                usecase_field_type(field),
                target_field.java_type,
                if target_field.optionality == crate::generate::Optionality::Nullable {
                    "?"
                } else {
                    ""
                }
            ));
        }
    }

    let mut expressions = Vec::with_capacity(target_fields.len());
    let mut default_imports = Vec::new();
    for field in &target_fields {
        if fields.iter().any(|input| input.name == field.name) {
            expressions.push(format!("command.{}()", field.name));
            continue;
        }
        let Some((expression, imports)) = usecase_default(root, domain, field) else {
            return Err(format!(
                "usecase {name} cannot safely infer `{}` ({}) for {target}.\n       fix: add `{}:<type>` to the usecase fields; Jails only infers ids, timestamps, status defaults, counters, flags, and empty optional/collection values.",
                field.name, field.java_type, field.name
            ));
        };
        expressions.push(expression);
        default_imports.extend(imports);
    }
    default_imports.sort();
    default_imports.dedup();

    let transactional = crate::pom::read(root).is_ok_and(|pom| {
        crate::pom::has_dependency(&pom, "org.springframework.boot", "spring-boot-starter-jdbc")
    });
    let main_service = crate::generate::main_dir(root, service);
    let test_service = crate::generate::test_dir(root, service);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    Ok(vec![
        (
            main_service.join(format!("{name}Command.java")),
            usecase_command_java(service, domain, name, fields),
            "usecase command",
        ),
        (
            main_service.join(format!("{name}UseCase.java")),
            usecase_port_java(service, domain, name, target),
            "usecase port",
        ),
        (
            main_service.join(format!("Default{name}UseCase.java")),
            usecase_impl_java(
                service,
                domain,
                app,
                name,
                target,
                &expressions,
                &default_imports,
                transactional,
            ),
            "usecase implementation",
        ),
        (
            test_service.join(format!("{name}UseCaseTest.java")),
            usecase_test_java(
                root,
                service,
                domain,
                adapters,
                name,
                target,
                fields,
                &target_fields,
                id,
            ),
            "usecase test",
        ),
        (
            main_web.join(format!("{name}Controller.java")),
            usecase_controller_java(security, service, web, target, name, fields),
            "usecase controller",
        ),
        (
            test_web.join(format!("{name}ControllerTest.java")),
            usecase_controller_test_java(
                root,
                security,
                service,
                web,
                domain,
                name,
                target,
                fields,
                &target_fields,
                crate::generate::webmvc_test_import(root),
            ),
            "usecase controller test",
        ),
    ])
}

fn usecase_normalized_type(java_type: &str) -> &str {
    match java_type {
        "Integer" => "int",
        "Long" => "long",
        "Double" => "double",
        "Float" => "float",
        "Boolean" => "boolean",
        "Short" => "short",
        "Byte" => "byte",
        "Character" => "char",
        other => other,
    }
}

fn usecase_field_type(field: &crate::generate::Field) -> String {
    if field.optionality == crate::generate::Optionality::Nullable {
        format!("Optional<{}>", field.java_type)
    } else {
        field.java_type.clone()
    }
}

fn usecase_default(
    root: &Path,
    domain: &str,
    field: &crate::generate::Field,
) -> Option<(String, Vec<String>)> {
    use crate::generate::Optionality;
    if field.optionality == Optionality::Nullable {
        return Some((
            "Optional.empty()".to_string(),
            vec!["java.util.Optional".to_string()],
        ));
    }
    if field.collection {
        let (expression, import) = if field.java_type.starts_with("Map") {
            ("Map.of()", "java.util.Map")
        } else {
            ("List.of()", "java.util.List")
        };
        return Some((expression.to_string(), vec![import.to_string()]));
    }
    match field.java_type.as_str() {
        "UUID" if field.name == "id" => Some((
            "UUID.randomUUID()".to_string(),
            vec!["java.util.UUID".to_string()],
        )),
        "String" if field.name == "id" => Some((
            "UUID.randomUUID().toString()".to_string(),
            vec!["java.util.UUID".to_string()],
        )),
        "Instant" => Some((
            "Instant.now()".to_string(),
            vec!["java.time.Instant".to_string()],
        )),
        "int" | "Integer" => Some(("0".to_string(), Vec::new())),
        "long" | "Long" => Some(("0L".to_string(), Vec::new())),
        "double" | "Double" => Some(("0.0d".to_string(), Vec::new())),
        "float" | "Float" => Some(("0.0f".to_string(), Vec::new())),
        "short" | "Short" => Some(("(short) 0".to_string(), Vec::new())),
        "byte" | "Byte" => Some(("(byte) 0".to_string(), Vec::new())),
        "boolean" | "Boolean" => Some(("false".to_string(), Vec::new())),
        owned if field.owned && field.name == "status" => {
            crate::generate::first_enum_constant(root, domain, owned).map(|_| {
                (
                    format!("{owned}.values()[0]"),
                    vec![format!("{domain}.{owned}")],
                )
            })
        }
        _ => None,
    }
}

fn usecase_command_java(
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let command = format!("{name}Command");
    let mut source = crate::generate::record_java(pkg, &command, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        source = source.replacen(&package, &format!("{package}\n{}\n", imports.join("\n")), 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {command} value."),
        &format!(" * Validated input for the {name} use case."),
    )
}

fn usecase_port_java(pkg: &str, domain: &str, name: &str, target: &str) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    format!(
        r#"package {pkg};

{target_import}/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface {name}UseCase {{

    {target} execute({name}Command command);
}}
"#
    )
}

fn usecase_impl_java(
    pkg: &str,
    domain: &str,
    app: &str,
    name: &str,
    target: &str,
    expressions: &[String],
    default_imports: &[String],
    transactional: bool,
) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = default_imports
        .iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let args = expressions
        .iter()
        .map(|expression| format!("                {expression}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = crate::generate::lower_first(target);
    let (transaction_import, annotation) = if transactional {
        (
            "import org.springframework.transaction.annotation.Transactional;\n",
            "    @Transactional\n",
        )
    } else {
        ("", "")
    };
    format!(
        r#"package {pkg};

{target_import}{repository_import}{imports}import java.util.Objects;
import org.springframework.stereotype.Component;
{transaction_import}
/** The conventional implementation generated from the target record's field model. */
@Component
public class Default{name}UseCase implements {name}UseCase {{

    private final {target}Repository repository;

    public Default{name}UseCase({target}Repository repository) {{
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }}

{annotation}    @Override
    public {target} execute({name}Command command) {{
        Objects.requireNonNull(command, "command is required");
        {target} {var} = new {target}(
{args});
        repository.save({var});
        return {var};
    }}
}}
"#
    )
}

fn java_literal_imports(fields: &[crate::generate::Field], domain: &str) -> Vec<String> {
    let mut imports = fields
        .iter()
        .flat_map(|field| field.imports.iter().map(|import| (*import).to_string()))
        .collect::<Vec<_>>();
    imports.extend(
        fields
            .iter()
            .filter(|field| field.owned)
            .map(|field| format!("{domain}.{}", field.java_type)),
    );
    if fields
        .iter()
        .any(|field| field.optionality == crate::generate::Optionality::Nullable)
    {
        imports.push("java.util.Optional".to_string());
    }
    imports.sort();
    imports.dedup();
    imports
}

fn usecase_test_java(
    root: &Path,
    pkg: &str,
    domain: &str,
    adapters: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
    id: &crate::generate::Field,
) -> String {
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Vec<_>>();
    let missing = fields
        .iter()
        .zip(&samples)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.name.as_str())
        .collect::<Vec<_>>();
    let args = fields
        .iter()
        .zip(samples)
        .map(|(field, sample)| {
            sample.unwrap_or_else(|| format!("null /* TODO: a {} */", field.java_type))
        })
        .collect::<Vec<_>>()
        .join(",\n                ");
    let copied = fields
        .iter()
        .map(|field| {
            format!(
                "        assertThat(created.{}()).isEqualTo(command.{}());",
                field.name, field.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let target_import = crate::generate::import_of(pkg, domain, target);
    let adapter_import =
        crate::generate::import_of(pkg, adapters, &format!("InMemory{target}Repository"));
    let disabled_import = if missing.is_empty() {
        ""
    } else {
        "import org.junit.jupiter.api.Disabled;\n"
    };
    let disabled = if missing.is_empty() {
        String::new()
    } else {
        format!(
            "@Disabled(\"todo: supply a sample for {} -- Jails cannot fabricate it\")\n",
            missing.join(", ")
        )
    };
    let id_assertion = if id.java_type == "String" {
        "        assertThat(created.id()).isNotBlank();"
    } else {
        "        assertThat(created.id()).isNotNull();"
    };
    let _ = target_fields;
    format!(
        r#"package {pkg};

{target_import}{adapter_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

{disabled}class {name}UseCaseTest {{

    private final InMemory{target}Repository repository = new InMemory{target}Repository();
    private final {name}UseCase useCase = new Default{name}UseCase(repository);

    @Test
    void createsAndPersistsTheResource() {{
        {name}Command command = new {name}Command(
                {args});

        {target} created = useCase.execute(command);

{id_assertion}
{copied}
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }}
}}
"#
    )
}

fn usecase_controller_java(
    security: &str,
    service: &str,
    web: &str,
    target: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let path = format!(
        "/actions/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    let resource_path = format!("/{}", crate::sql::table_name(target).replace('_', "-"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "command");
    format!(
        r#"package {web};

{command_import}{usecase_import}{scope_import}import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping({name}Controller.PATH)
public final class {name}Controller {{

    public static final String PATH = "{path}";
    private static final String RESOURCE_PATH = "{resource_path}";

    private final {name}UseCase useCase;
{scope_field}

    public {name}Controller({name}UseCase useCase{scope_constructor}) {{
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
{scope_assignment}
    }}

    @PostMapping
    public ResponseEntity<{target}Response> execute(
            @Valid @RequestBody {name}Command command{scope_parameter}) {{
{scope_checks}
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body({target}Response.from(created));
    }}
}}
"#
    )
}

fn json_sample(root: &Path, domain: &str, field: &crate::generate::Field) -> Option<String> {
    if field.optionality == crate::generate::Optionality::Nullable {
        return Some("null".to_string());
    }
    if field.collection {
        return Some(if field.java_type.starts_with("Map") {
            "{}".to_string()
        } else {
            "[]".to_string()
        });
    }
    let quoted = match field.java_type.as_str() {
        "String" => Some("sample".to_string()),
        "UUID" => Some("00000000-0000-0000-0000-000000000001".to_string()),
        "Instant" => Some("2024-01-01T00:00:00Z".to_string()),
        "LocalDate" => Some("2024-01-01".to_string()),
        "LocalDateTime" => Some("2024-01-01T00:00:00".to_string()),
        "Duration" => Some("PT1M".to_string()),
        "URI" => Some("https://example.test/items/1".to_string()),
        "Path" => Some("/tmp/example".to_string()),
        "ZoneId" => Some("UTC".to_string()),
        owned if field.owned => crate::generate::first_enum_constant(root, domain, owned),
        _ => None,
    };
    if let Some(value) = quoted {
        return Some(format!("\"{value}\""));
    }
    match field.java_type.as_str() {
        "int" | "Integer" => Some("7".to_string()),
        "long" | "Long" => Some("7".to_string()),
        "double" | "Double" | "float" | "Float" | "BigDecimal" => Some("12.5".to_string()),
        "boolean" | "Boolean" => Some("true".to_string()),
        _ => None,
    }
}

fn usecase_controller_test_java(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
    webmvc_test_import: &str,
) -> String {
    let json = fields
        .iter()
        .map(|field| {
            json_sample(root, domain, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled_reason = if json.is_none() {
        Some("Jails cannot serialize one of the command field samples")
    } else if target_samples.is_none() {
        Some("Jails cannot construct the target resource sample")
    } else {
        None
    };
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let target_import = crate::generate::import_of(web, domain, target);
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let (disabled_import, disabled) = match disabled_reason {
        Some(reason) => (
            "import org.junit.jupiter.api.Disabled;\n",
            format!("    @Disabled(\"todo: {reason}\")\n"),
        ),
        None => ("", String::new()),
    };
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    format!(
        r#"package {web};

{command_import}{usecase_import}{target_import}{scope_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {webmvc_test_import};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({name}Controller.class)
@Import({name}ControllerTest.Config.class)
class {name}ControllerTest {{

    @Autowired
    private MockMvcTester mvc;

{disabled}    @Test
    void postExecutesTheUseCase() {{
        assertThat(mvc.post()
                .uri({name}Controller.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{{
{json}
}}
"""))
                .hasStatus(201);
    }}

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {{

        @Bean
        {name}UseCase useCase() {{
            return command -> new {target}(
                    {target_args});
        }}
{scope_bean}
    }}
}}
"#
    )
}

#[cfg(test)]
mod usecase_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-usecase-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo/domain")).unwrap();
        root
    }

    fn write_record(root: &Path, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("src/main/java/com/example/demo/domain/{name}.java")),
            crate::generate::record_java("com.example.demo.domain", name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn usecase_derives_only_conservative_defaults_and_persists_the_result() {
        let root = scratch("defaults");
        std::fs::write(
            root.join("pom.xml"),
            r#"<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency></dependencies></project>"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/domain/CrawlStatus.java"),
            "package com.example.demo.domain;\npublic enum CrawlStatus { QUEUED, RUNNING }\n",
        )
        .unwrap();
        write_record(
            &root,
            "CrawlRun",
            &[
                "id:uuid",
                "seedUrl:uri",
                "status:CrawlStatus",
                "pagesVisited:long",
                "startedAt:instant?",
            ],
        );
        let fields = crate::generate::parse_fields(&["seedUrl:uri".to_string()]).unwrap();

        let files = usecase_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "QueueCrawl",
            "CrawlRun",
            &fields,
        )
        .unwrap();
        let implementation = &files
            .iter()
            .find(|(_, _, kind)| *kind == "usecase implementation")
            .unwrap()
            .1;

        assert!(
            implementation.contains("UUID.randomUUID()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("command.seedUrl()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("CrawlStatus.values()[0]"),
            "{implementation}"
        );
        assert!(implementation.contains("0L"), "{implementation}");
        assert!(
            implementation.contains("Optional.empty()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("repository.save(crawlRun)"),
            "{implementation}"
        );
        assert!(
            implementation.contains("@Transactional"),
            "{implementation}"
        );
        assert!(!implementation.contains("final class"), "{implementation}");
        assert!(!implementation.contains("TODO"), "{implementation}");
    }

    #[test]
    fn usecase_refuses_to_invent_a_foreign_identity() {
        let root = scratch("foreign-id");
        write_record(&root, "Membership", &["id:uuid", "workspaceId:uuid"]);

        let error = usecase_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "CreateMembership",
            "Membership",
            &[],
        )
        .unwrap_err();

        assert!(
            error.contains("cannot safely infer `workspaceId`"),
            "{error}"
        );
    }

    #[test]
    fn usecase_rejects_input_that_the_target_cannot_store() {
        let root = scratch("unknown-input");
        write_record(&root, "Workspace", &["id:uuid", "name:string"]);
        let fields = crate::generate::parse_fields(&["slug:string".to_string()]).unwrap();

        let error = usecase_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "CreateWorkspace",
            "Workspace",
            &fields,
        )
        .unwrap_err();

        assert!(error.contains("Workspace has no component"), "{error}");
    }
}

// ---------------------------------------------------------------------------
// `generate transition` -- scope-safe optimistic updates in PostgreSQL.
// ---------------------------------------------------------------------------

pub(crate) fn transition_files(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    app: &str,
    adapters: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    require_scope_authorizer(root, security, "transition", name, fields)?;
    let target_fields = crate::generate::fields_from_record(root, domain, target).ok_or_else(|| {
        format!("transition {name} targets {target}, but no record components could be read from {target}.java")
    })?;
    if fields.iter().any(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "transition {name} accepts required scalar fields only so match and update semantics stay exact"
        ));
    }
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "transition {name} declares `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "transition {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            ));
        }
    }
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("transition {name} needs the target's required `id` field"))?;
    let version = fields
        .iter()
        .find(|field| field.name == "version")
        .ok_or_else(|| format!("transition {name} needs a required numeric `version` field"))?;
    if !matches!(usecase_normalized_type(&version.java_type), "long" | "int") {
        return Err(format!(
            "transition {name} needs `version:long` or `version:int`, not version:{}",
            version.java_type
        ));
    }
    let update_fields = fields
        .iter()
        .filter(|field| {
            field.name != id.name && field.name != version.name && !field.constraints.scoped
        })
        .collect::<Vec<_>>();
    if update_fields.is_empty() {
        return Err(format!(
            "transition {name} needs at least one field to update in addition to id, @scope fields, and version"
        ));
    }
    let target_columns = crate::sql::columns(&target_fields, root, domain, "rows");
    let command_columns = crate::sql::columns(fields, root, domain, "command");
    if target_columns
        .iter()
        .chain(command_columns.iter())
        .any(|column| !column.mapped())
    {
        return Err(format!(
            "transition {name} contains a field Jails cannot map to JDBC"
        ));
    }
    let main_service = crate::generate::main_dir(root, service);
    let main_adapters = crate::generate::main_dir(root, adapters);
    let test_adapters = crate::generate::test_dir(root, adapters);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    Ok(vec![
        (
            main_service.join(format!("{name}Command.java")),
            usecase_command_java(service, domain, name, fields),
            "transition command",
        ),
        (
            main_service.join(format!("{name}UseCase.java")),
            transition_port_java(service, domain, name, target),
            "transition port",
        ),
        (
            main_adapters.join(format!("Jdbc{name}Transition.java")),
            jdbc_transition_java(
                adapters,
                service,
                domain,
                name,
                target,
                fields,
                &target_columns,
                &command_columns,
                &update_fields,
            ),
            "optimistic JDBC transition",
        ),
        (
            test_adapters.join(format!("Jdbc{name}TransitionIT.java")),
            jdbc_transition_it_java(
                root,
                adapters,
                service,
                domain,
                app,
                name,
                target,
                fields,
                &target_fields,
            ),
            "optimistic transition integration test",
        ),
        (
            main_web.join(format!("{name}Controller.java")),
            transition_controller_java(security, service, web, name, target, fields),
            "transition controller",
        ),
        (
            test_web.join(format!("{name}ControllerTest.java")),
            transition_controller_test_java(
                root,
                security,
                service,
                web,
                domain,
                name,
                target,
                fields,
                &target_fields,
                crate::generate::webmvc_test_import(root),
            ),
            "transition controller test",
        ),
    ])
}

fn transition_port_java(pkg: &str, domain: &str, name: &str, target: &str) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    format!(
        r#"package {pkg};

{target_import}/** Atomic state change guarded by tenant scope and an optimistic version. */
@FunctionalInterface
public interface {name}UseCase {{

    {target} execute({name}Command command);

    final class NotFoundException extends RuntimeException {{
        public NotFoundException() {{ super("resource not found in the authorized scope"); }}
    }}

    final class StaleVersionException extends RuntimeException {{
        public StaleVersionException() {{ super("resource version is stale"); }}
    }}
}}
"#
    )
}

fn jdbc_transition_java(
    pkg: &str,
    service: &str,
    domain: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_columns: &[crate::sql::Column],
    command_columns: &[crate::sql::Column],
    update_fields: &[&crate::generate::Field],
) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    let command_import = crate::generate::import_of(pkg, service, &format!("{name}Command"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}UseCase"));
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .chain(crate::sql::imports(command_columns))
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    if target_columns.iter().any(|column| {
        column
            .read
            .as_deref()
            .is_some_and(|read| read.contains("Optional."))
    }) {
        imports.push_str("import java.util.Optional;\n");
    }
    for column in target_columns.iter().chain(command_columns.iter()) {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let assignments = update_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .chain(std::iter::once("version = version + 1".to_string()))
        .collect::<Vec<_>>()
        .join(",\n                            ");
    let match_fields = fields
        .iter()
        .filter(|field| field.name == "id" || field.constraints.scoped)
        .collect::<Vec<_>>();
    let optimistic_predicates = match_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .chain(std::iter::once("version = :version".to_string()))
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let existence_predicates = match_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .collect::<Vec<_>>()
        .join("\n                                  and ");
    let bindings_for = |selected: &[&crate::generate::Field], indent: &str| {
        selected
            .iter()
            .map(|field| {
                let column = command_columns
                    .iter()
                    .find(|column| column.name == crate::sql::snake_case(&field.name))
                    .expect("validated transition column");
                format!(
                    "{indent}.param(\"{}\", {})",
                    column.name,
                    column.write.as_deref().expect("mapped transition column")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let all = fields.iter().collect::<Vec<_>>();
    let update_bindings = bindings_for(&all, "                ");
    let existence_bindings = bindings_for(&match_fields, "                ");
    let select = target_columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = target_columns
        .iter()
        .map(|column| format!("                {}", column.read.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .join(",\n");
    let table = crate::sql::table_name(target);
    format!(
        r#"package {pkg};

{target_import}{command_import}{port_import}{imports}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap: scoped matches cannot mutate another tenant's row. */
@Component
public class Jdbc{name}Transition implements {name}UseCase {{

    private final JdbcClient db;

    public Jdbc{name}Transition(JdbcClient db) {{
        this.db = Objects.requireNonNull(db, "db is required");
    }}

    @Override
    @Transactional
    public {target} execute({name}Command command) {{
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update {table}
                        set {assignments}
                        where {optimistic_predicates}
                        returning {select}
                        """)
{update_bindings}
                .query(Jdbc{name}Transition::map)
                .optional();
        if (updated.isPresent()) return updated.orElseThrow();

        boolean existsInScope = db.sql("""
                        select exists(
                            select 1 from {table}
                            where {existence_predicates}
                        )
                        """)
{existence_bindings}
                .query(Boolean.class)
                .single();
        if (existsInScope) throw new {name}UseCase.StaleVersionException();
        throw new {name}UseCase.NotFoundException();
    }}

    private static {target} map(ResultSet rows, int rowNumber) throws SQLException {{
        return new {target}(
{map_args});
    }}
}}
"#
    )
}

fn transition_controller_java(
    security: &str,
    service: &str,
    web: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "command");
    let path = format!(
        "/actions/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    format!(
        r#"package {web};

{command_import}{usecase_import}{scope_import}import jakarta.validation.Valid;
import java.util.Objects;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

/** HTTP adapter for one optimistic state transition. */
@RestController
@RequestMapping({name}Controller.PATH)
public final class {name}Controller {{

    public static final String PATH = "{path}";
    private final {name}UseCase useCase;
{scope_field}

    public {name}Controller({name}UseCase useCase{scope_constructor}) {{
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
{scope_assignment}
    }}

    @PutMapping
    public {target}Response execute(
            @Valid @RequestBody {name}Command command{scope_parameter}) {{
{scope_checks}
        try {{
            return {target}Response.from(useCase.execute(command));
        }} catch ({name}UseCase.NotFoundException missing) {{
            throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);
        }} catch ({name}UseCase.StaleVersionException stale) {{
            throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);
        }}
    }}
}}
"#
    )
}

fn jdbc_transition_it_java(
    root: &Path,
    pkg: &str,
    service: &str,
    domain: &str,
    app: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
) -> String {
    let command_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = command_samples.is_none() || target_samples.is_none();
    let command_values = command_samples.unwrap_or_default();
    let command_args = command_values.join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let wrong_scope_test = fields
        .iter()
        .enumerate()
        .find_map(|(index, field)| {
            field
                .constraints
                .scoped
                .then(|| durable_alternate_sample(field).map(|value| (index, value)))
                .flatten()
        })
        .map_or_else(String::new, |(changed, alternate)| {
            let args = command_values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if index == changed {
                        alternate.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n                ");
            format!(
                r#"
    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {{
        var stored = new {target}(
                {target_args});
        repository.save(stored);
        var wrongScope = new {name}Command(
                {args});

        assertThatThrownBy(() -> useCase.execute(wrongScope))
                .isInstanceOf({name}UseCase.NotFoundException.class);
        assertThat(repository.findById(String.valueOf(stored.id()))).contains(stored);
    }}
"#
            )
        });
    let target_import = crate::generate::import_of(pkg, domain, target);
    let command_import = crate::generate::import_of(pkg, service, &format!("{name}Command"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}UseCase"));
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply transition samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    format!(
        r#"package {pkg};

{target_import}{command_import}{port_import}{repository_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

{annotation}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{name}TransitionIT {{

    @Autowired private {target}Repository repository;
    @Autowired private {name}UseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {{
        repository.save(new {target}(
                {target_args}));
        var command = new {name}Command(
                {command_args});

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf({name}UseCase.StaleVersionException.class);
        assertThat(repository.findById(String.valueOf(command.id())))
                .get().extracting({target}::version)
                .isEqualTo(updated.version());
    }}
{wrong_scope_test}
}}
"#
    )
}

fn transition_controller_test_java(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
    webmvc_test_import: &str,
) -> String {
    let json = fields
        .iter()
        .map(|field| {
            json_sample(root, domain, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let target_import = crate::generate::import_of(web, domain, target);
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "    @Disabled(\"todo: supply transition samples\")\n"
    } else {
        ""
    };
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    format!(
        r#"package {web};

{command_import}{usecase_import}{target_import}{scope_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {webmvc_test_import};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({name}Controller.class)
@Import({name}ControllerTest.Config.class)
class {name}ControllerTest {{

    @Autowired private MockMvcTester mvc;

{annotation}    @Test
    void putExecutesTheTransition() {{
        assertThat(mvc.put().uri({name}Controller.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{{
{json}
}}
"""))
                .hasStatusOk();
    }}

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {{
        @Bean
        {name}UseCase useCase() {{
            return command -> new {target}(
                    {target_args});
        }}
{scope_bean}    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// `generate query` -- typed equality filters executed by PostgreSQL.
// ---------------------------------------------------------------------------

pub(crate) fn query_files(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    app: &str,
    adapters: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    require_scope_authorizer(root, security, "query", name, fields)?;
    if fields.is_empty() {
        return Err(format!(
            "query {name} needs at least one typed filter; use the scaffold's list endpoint for an unfiltered read"
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "query {name} filter `{}` is optional or a collection. This first query contract only accepts required scalar equality filters so null/list semantics are never guessed.",
            field.name
        ));
    }
    let target_fields = crate::generate::fields_from_record(root, domain, target).ok_or_else(|| {
        format!("query {name} targets {target}, but no record components could be read from {target}.java")
    })?;
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "query {name} filters `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "query {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            ));
        }
    }
    let target_columns = crate::sql::columns(&target_fields, root, domain, "row");
    let filter_columns = crate::sql::columns(fields, root, domain, "query");
    let unmapped = target_columns
        .iter()
        .chain(filter_columns.iter())
        .filter(|column| !column.mapped())
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return Err(format!(
            "query {name} cannot map database column(s): {}. Model collections/owned values separately or add an explicit mapping before generating the query.",
            unmapped.join(", ")
        ));
    }
    let main_service = crate::generate::main_dir(root, service);
    let main_adapters = crate::generate::main_dir(root, adapters);
    let test_adapters = crate::generate::test_dir(root, adapters);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    Ok(vec![
        (
            main_service.join(format!("{name}Query.java")),
            query_record_java(service, domain, name, fields),
            "query input",
        ),
        (
            main_service.join(format!("{name}QueryPort.java")),
            query_port_java(service, domain, name, target),
            "query port",
        ),
        (
            main_adapters.join(format!("Jdbc{name}Query.java")),
            jdbc_query_java(
                adapters,
                service,
                domain,
                name,
                target,
                &target_columns,
                &filter_columns,
            ),
            "JDBC query adapter",
        ),
        (
            test_adapters.join(format!("Jdbc{name}QueryIT.java")),
            jdbc_query_it_java(
                root,
                adapters,
                service,
                domain,
                app,
                name,
                target,
                fields,
                &target_fields,
            ),
            "JDBC query integration test",
        ),
        (
            main_web.join(format!("{name}QueryController.java")),
            query_controller_java(security, service, web, name, target, fields),
            "query controller",
        ),
        (
            test_web.join(format!("{name}QueryControllerTest.java")),
            query_controller_test_java(
                root,
                security,
                service,
                web,
                domain,
                name,
                target,
                fields,
                &target_fields,
                crate::generate::webmvc_test_import(root),
            ),
            "query controller test",
        ),
    ])
}

fn query_record_java(
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let class = format!("{name}Query");
    let mut source = crate::generate::record_java(pkg, &class, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        source = source.replacen(&package, &format!("{package}\n{}\n", imports.join("\n")), 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {class} value."),
        &format!(" * Typed filters for the {name} query."),
    )
}

fn query_port_java(pkg: &str, domain: &str, name: &str, target: &str) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    format!(
        r#"package {pkg};

{target_import}import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface {name}QueryPort {{

    List<{target}> execute({name}Query query);
}}
"#
    )
}

fn jdbc_query_java(
    pkg: &str,
    service: &str,
    domain: &str,
    name: &str,
    target: &str,
    target_columns: &[crate::sql::Column],
    filter_columns: &[crate::sql::Column],
) -> String {
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .chain(crate::sql::imports(filter_columns))
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    if target_columns.iter().any(|column| {
        column
            .read
            .as_deref()
            .is_some_and(|read| read.contains("Optional."))
    }) {
        imports.push_str("import java.util.Optional;\n");
    }
    for column in target_columns.iter().chain(filter_columns.iter()) {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let select = target_columns
        .iter()
        .map(|column| format!("            {},", column.name))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches(',')
        .to_string();
    let predicates = filter_columns
        .iter()
        .map(|column| format!("{} = :{}", column.name, column.name))
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let bindings = filter_columns
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped query column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_args = target_columns
        .iter()
        .map(|column| {
            format!(
                "                {}",
                column.read.as_deref().expect("mapped target column")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let table = crate::sql::table_name(target);
    let order = target_columns
        .iter()
        .find(|column| column.name == "id")
        .map(|column| column.name.as_str())
        .unwrap_or(&target_columns[0].name);
    format!(
        r#"package {pkg};

{target_import}{query_import}{port_import}{imports}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class Jdbc{name}Query implements {name}QueryPort {{

    private static final String COLUMNS =
            """
{select}
            """;

    private final JdbcClient db;

    public Jdbc{name}Query(JdbcClient db) {{
        this.db = Objects.requireNonNull(db, "db is required");
    }}

    @Override
    public List<{target}> execute({name}Query query) {{
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from {table}
                        where {predicates}
                        order by {order}
                        """.formatted(COLUMNS))
{bindings}
                .query(Jdbc{name}Query::map)
                .list();
    }}

    private static {target} map(ResultSet rows, int rowNumber) throws SQLException {{
        return new {target}(
{map_args});
    }}
}}
"#
    )
}

fn jdbc_query_it_java(
    root: &Path,
    pkg: &str,
    service: &str,
    domain: &str,
    app: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
) -> String {
    let query_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = query_samples.is_none() || target_samples.is_none();
    let query_args = query_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    format!(
        r#"package {pkg};

{target_import}{query_import}{port_import}{repository_import}{imports}{disabled_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;

{annotation}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{name}QueryIT {{

    @Autowired
    private {target}Repository repository;

    @Autowired
    private {name}QueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {{
        {target} stored = new {target}(
                {target_args});
        repository.save(stored);

        var found = queryPort.execute(new {name}Query(
                {query_args}));

        assertThat(found).contains(stored);
    }}
}}
"#
    )
}

fn query_controller_java(
    security: &str,
    service: &str,
    web: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let query_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
    let path = format!(
        "/queries/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "query");
    format!(
        r#"package {web};

{query_import}{port_import}{scope_import}import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping({name}QueryController.PATH)
public final class {name}QueryController {{

    public static final String PATH = "{path}";

    private final {name}QueryPort queryPort;
{scope_field}

    public {name}QueryController({name}QueryPort queryPort{scope_constructor}) {{
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
{scope_assignment}
    }}

    @PostMapping
    public List<{target}Response> execute(
            @Valid @RequestBody {name}Query query{scope_parameter}) {{
{scope_checks}
        return queryPort.execute(query).stream().map({target}Response::from).toList();
    }}
}}
"#
    )
}

fn query_controller_test_java(
    root: &Path,
    security: &str,
    service: &str,
    web: &str,
    domain: &str,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    target_fields: &[crate::generate::Field],
    webmvc_test_import: &str,
) -> String {
    let json = fields
        .iter()
        .map(|field| {
            json_sample(root, domain, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
    let target_import = crate::generate::import_of(web, domain, target);
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "    @Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    format!(
        r#"package {web};

{port_import}{target_import}{scope_import}{imports}{disabled_import}import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {webmvc_test_import};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({name}QueryController.class)
@Import({name}QueryControllerTest.Config.class)
class {name}QueryControllerTest {{

    @Autowired
    private MockMvcTester mvc;

{annotation}    @Test
    void postExecutesTheDatabaseQueryPort() {{
        assertThat(mvc.post()
                .uri({name}QueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{{
{json}
}}
"""))
                .hasStatusOk()
                .bodyJson();
    }}

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {{

        @Bean
        {name}QueryPort queryPort() {{
            return query -> List.of(new {target}(
                    {target_args}));
        }}
{scope_bean}
    }}
}}
"#
    )
}

#[cfg(test)]
mod query_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-query-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo/domain")).unwrap();
        root
    }

    fn write_record(root: &Path, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("src/main/java/com/example/demo/domain/{name}.java")),
            crate::generate::record_java("com.example.demo.domain", name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn query_generates_visible_named_parameter_sql_and_real_database_test() {
        let root = scratch("sql");
        write_record(
            &root,
            "Message",
            &[
                "id:uuid",
                "conversationId:uuid",
                "body:string!",
                "createdAt:instant",
            ],
        );
        let fields = crate::generate::parse_fields(&["conversationId:uuid".to_string()]).unwrap();

        let files = query_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "MessagesByConversation",
            "Message",
            &fields,
        )
        .unwrap();
        let adapter = &files
            .iter()
            .find(|(_, _, kind)| *kind == "JDBC query adapter")
            .unwrap()
            .1;
        let integration_test = &files
            .iter()
            .find(|(_, _, kind)| *kind == "JDBC query integration test")
            .unwrap()
            .1;

        assert!(
            adapter.contains("where conversation_id = :conversation_id"),
            "{adapter}"
        );
        assert!(adapter.contains(".param(\"conversation_id\""), "{adapter}");
        assert!(adapter.contains("order by id"), "{adapter}");
        assert!(
            integration_test.contains("repository.save(stored)"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("contains(stored)"),
            "{integration_test}"
        );
    }

    #[test]
    fn query_rejects_an_unfiltered_read_instead_of_guessing_pagination() {
        let root = scratch("empty");
        write_record(&root, "Contact", &["id:uuid", "workspaceId:uuid"]);

        let error = query_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "Contacts",
            "Contact",
            &[],
        )
        .unwrap_err();

        assert!(error.contains("at least one typed filter"), "{error}");
    }

    #[test]
    fn query_rejects_nullable_filters_instead_of_inventing_null_semantics() {
        let root = scratch("nullable");
        write_record(&root, "Contact", &["id:uuid", "email:string?"]);
        let fields = crate::generate::parse_fields(&["email:string?".to_string()]).unwrap();

        let error = query_files(
            &root,
            "com.example.demo",
            "com.example.demo.service",
            "com.example.demo.web",
            "com.example.demo.domain",
            "com.example.demo.app",
            "com.example.demo.adapters",
            "ContactsByEmail",
            "Contact",
            &fields,
        )
        .unwrap_err();

        assert!(
            error.contains("null/list semantics are never guessed"),
            "{error}"
        );
    }
}

// ---------------------------------------------------------------------------
// `generate event` -- a Kafka publisher, listener and payload, as one slice.
// ---------------------------------------------------------------------------

/// Boot's Testcontainers integration, needed for `@ServiceConnection`.
pub(crate) const SPRING_TESTCONTAINERS: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-testcontainers",
    version: None,
    scope: Some("test"),
    optional: false,
};

/// Testcontainers' Kafka module. Named the 2.x way (`testcontainers-kafka`),
/// matching the postgres module `add db` already pins.
pub(crate) const TESTCONTAINERS_KAFKA: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-kafka",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};

/// The Spring Kafka properties that make publish-and-consume actually work.
///
/// Every one of these is a thing people discover by losing an afternoon:
///
/// - `auto-offset-reset=earliest`: a consumer joining a group for the first
///   time otherwise starts at the *end* of the topic, so anything published
///   before it joined is invisible. This is the single most common reason a
///   Kafka integration test hangs and then fails with nothing consumed.
/// - `JacksonJsonSerializer`/`JacksonJsonDeserializer`: the defaults are
///   `StringSerializer`, so a record payload arrives as its `toString()` and
///   comes back unparseable. Note the `Jackson` prefix -- the older
///   `JsonSerializer`/`JsonDeserializer` pair is deprecated for removal since
///   Spring Kafka 4.0, which moved to Jackson 3.
/// - `spring.json.trusted.packages`: the deserializer refuses to instantiate
///   a type outside the trusted list, and reports it as a deserialization
///   failure rather than a configuration one.
pub(crate) fn kafka_properties(base: &str, group: &str) -> Vec<String> {
    vec![
        "spring.kafka.bootstrap-servers=localhost:9092".to_string(),
        format!("spring.kafka.consumer.group-id={group}"),
        "spring.kafka.consumer.auto-offset-reset=earliest".to_string(),
        "spring.kafka.producer.value-serializer=org.springframework.kafka.support.serializer.JacksonJsonSerializer".to_string(),
        // Both the base package *and* a wildcard for everything under it.
        // The check is `PatternMatchUtils.simpleMatch` against the class's
        // package name, so it is neither a prefix match nor recursive:
        // `com.example.app` alone rejects `com.example.app.messaging` --
        // where `jails g event` puts the payload -- and the failure surfaces
        // as a SerializationException reading "is not in the trusted
        // packages", which sounds like a security setting rather than a
        // missing dot-star. The wildcard alone would not match the base
        // package itself, hence both.
        format!("spring.kafka.consumer.properties.spring.json.trusted.packages={base},{base}.*"),
        // KIP-848. The broker default since Kafka 4.0, but the *client*
        // default is still `classic`, so a project that does not opt in keeps
        // the stop-the-world rebalance -- every consumer in the group stops
        // while one joins. Nothing reports this; it just stays slow.
        "spring.kafka.consumer.properties.group.protocol=consumer".to_string(),
        // Durability over throughput. `acks=all` waits for the in-sync
        // replicas, and idempotence stops a producer retry from writing the
        // record twice. Both are stated rather than inherited because the
        // defaults have moved between client versions.
        "spring.kafka.producer.acks=all".to_string(),
        "spring.kafka.producer.properties.enable.idempotence=true".to_string(),
    ]
    .into_iter()
    .chain(kafka_deserializer_properties())
    .collect()
}

/// The deserializer half, kept apart because it is what makes a poison
/// message survivable.
///
/// A record that will not deserialize will not deserialize on the next
/// attempt either. Left as a plain `JacksonJsonDeserializer`, the failure is
/// thrown *inside* the consumer before any error handler can see it as a
/// record, so the container retries the same bad offset forever and the
/// partition stops. `ErrorHandlingDeserializer` catches it and hands the
/// error along as the record's value, which is the only shape
/// `DefaultErrorHandler` can route to a dead-letter topic.
///
/// Separate from [`kafka_properties`] only for readability -- `add kafka`
/// writes both, and one without the other is the bug.
fn kafka_deserializer_properties() -> Vec<String> {
    vec![
        "spring.kafka.consumer.value-deserializer=org.springframework.kafka.support.serializer.ErrorHandlingDeserializer".to_string(),
        "spring.kafka.consumer.properties.spring.deserializer.value.delegate.class=org.springframework.kafka.support.serializer.JacksonJsonDeserializer".to_string(),
    ]
}

/// What happens to a record that does not process cleanly.
///
/// This is the half of a Kafka integration that nobody writes on day one and
/// everybody needs on day two. Spring Kafka's default is to retry a failing
/// record ten times and then *log and move on*, and the shape of the failure
/// decides which half of that is wrong:
///
/// - A record that will not deserialize, or that names an enum constant this
///   service does not have, fails identically on every attempt. Retrying it
///   is a loop that costs the whole partition, and the only symptom is
///   consumer lag with no new errors after the first.
/// - A database that is briefly unavailable is the opposite case, and the one
///   the backoff exists for.
///
/// So the classification is the load-bearing part, not the backoff.
///
/// It is expressed as *one* marker exception rather than a list of JDK types,
/// for two reasons that come out of
/// `deps/spring-kafka/.../listener/ExceptionClassifier.java`:
///
/// - The framework already treats `DeserializationException`,
///   `MessageConversionException`, `ConversionException`,
///   `MethodArgumentResolutionException` and `ClassCastException` as fatal
///   (`defaultFatalExceptionsList`). Re-listing one of those reads as if the
///   generated list were the whole policy, and hides the other four.
/// - Naming `NullPointerException` there is worse than redundant. An NPE is a
///   bug in the listener, not a bad record; classifying it permanent commits
///   the offset and destroys the repeating failure that would have surfaced
///   it. Only the domain knows what is genuinely unprocessable, so only the
///   domain gets to say so -- see [`non_retryable_exception_java`].
///
/// No `NewTopic` beans here: `add kafka` does not know what this service's
/// topics are called. `jails g event <Name>` declares them, because it does.
fn kafka_config_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/kafka_config_java.java"),
        &[("pkg", pkg)],
    )
}

/// The domain's own "no retry will ever fix this".
///
/// Deliberately unlike [`api_exception_java`], which is sealed, abstract and
/// stack-trace-free. This one is open -- callers throw and subclass it -- and it
/// keeps its stack trace, because it wraps a real cause and that cause is what
/// a human reads out of the dead-letter headers.
fn non_retryable_exception_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/non_retryable_exception_java.java"),
        &[("pkg", pkg)],
    )
}

/// A test that the poison-message path is actually wired, without a broker.
///
/// The container-backed version belongs to `g event`; this one exists so that
/// `add kafka` keeps the promise `jails add --help` makes -- a dependency,
/// the code that uses it, *and a test that proves it works*.
fn kafka_config_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/kafka_config_test_java.java"),
        &[("pkg", pkg)],
    )
}

/// The files `add kafka` writes on a Spring project.
pub(crate) fn kafka_files(
    root: &Path,
    pkg: &str,
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    vec![
        (
            crate::generate::main_dir(root, pkg).join("KafkaConfig.java"),
            kafka_config_java(pkg),
            "kafka config",
        ),
        (
            crate::generate::main_dir(root, pkg).join("NonRetryableException.java"),
            non_retryable_exception_java(pkg),
            "non-retryable exception",
        ),
        (
            crate::generate::test_dir(root, pkg).join("KafkaConfigTest.java"),
            kafka_config_test_java(pkg),
            "kafka config test",
        ),
    ]
}

pub(crate) fn event_files(
    root: &Path,
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> Result<Vec<(std::path::PathBuf, String, &'static str)>> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let topic = crate::sql::snake_case(name).replace('_', "-");
    let id = fields.iter().find(|field| field.name == "id");
    if !fields.is_empty() && id.is_none() {
        return Err(format!(
            "an event payload needs a stable `id` field for deduplication and Kafka partitioning.\n       \
             Add `id:string!` or `id:uuid` to `jails g event {name} ...`."
        ));
    }
    if id.is_some_and(|field| field.optionality == crate::generate::Optionality::Nullable) {
        return Err(
            "an event `id` cannot be optional: a null key loses per-entity ordering".to_string(),
        );
    }
    let key = id
        .filter(|field| field.java_type != "String")
        .map(|_| "String.valueOf(event.id())")
        .unwrap_or("event.id()");
    Ok(vec![
        (
            main.join(format!("{name}Event.java")),
            event_java(pkg, domain, name, fields),
            "event",
        ),
        (
            main.join(format!("{name}Publisher.java")),
            publisher_java(pkg, name, &topic, key),
            "publisher",
        ),
        (
            main.join(format!("{name}Listener.java")),
            listener_java(pkg, name, &topic),
            "listener",
        ),
        (
            test.join(format!("{name}MessagingIT.java")),
            messaging_it_java(root, pkg, domain, name, &topic, fields),
            "messaging integration test",
        ),
    ])
}

fn event_java(pkg: &str, domain: &str, name: &str, fields: &[crate::generate::Field]) -> String {
    if fields.is_empty() {
        return crate::template::render(
            include_str!("../templates/spring/event_java.java"),
            &[("pkg", pkg), ("name", name)],
        );
    }

    let event = format!("{name}Event");
    let mut source = crate::generate::record_java(pkg, &event, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        let replacement = format!("{package}\n{}\n", imports.join("\n"));
        source = source.replacen(&package, &replacement, 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {event} value."),
        &format!(" * Immutable payload published as {event}."),
    )
}

fn publisher_java(pkg: &str, name: &str, topic: &str, key: &str) -> String {
    let source = crate::template::render(
        include_str!("../templates/spring/publisher_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    );
    source.replace(
        "kafka.send(topic, event.id(), event)",
        &format!("kafka.send(topic, {key}, event)"),
    )
}

fn listener_java(pkg: &str, name: &str, topic: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/listener_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    )
}

fn messaging_it_java(
    root: &Path,
    pkg: &str,
    domain: &str,
    name: &str,
    topic: &str,
    fields: &[crate::generate::Field],
) -> String {
    let (event_imports, disabled_import, disabled, event_args, expected_id) = if fields.is_empty() {
        (
            "import java.time.Instant;\n".to_string(),
            String::new(),
            String::new(),
            "\"probe-1\", Instant.parse(\"2024-01-01T00:00:00Z\")".to_string(),
            "\"probe-1\"".to_string(),
        )
    } else {
        let samples = fields
            .iter()
            .map(|field| crate::generate::sample_value(field, root, domain))
            .collect::<Vec<_>>();
        let missing = fields
            .iter()
            .zip(&samples)
            .filter(|(_, sample)| sample.is_none())
            .map(|(field, _)| field.name.as_str())
            .collect::<Vec<_>>();
        let expected_id = fields
            .iter()
            .zip(&samples)
            .find(|(field, _)| field.name == "id")
            .and_then(|(_, sample)| sample.clone())
            .unwrap_or_else(|| "null /* TODO: an event id sample */".to_string());
        let event_args = fields
            .iter()
            .zip(samples)
            .map(|(field, sample)| {
                sample.unwrap_or_else(|| format!("null /* TODO: a {} */", field.java_type))
            })
            .collect::<Vec<_>>()
            .join(",\n                ");
        let mut imports = fields
            .iter()
            .flat_map(|field| field.imports.iter().copied().map(str::to_string))
            .collect::<Vec<_>>();
        imports.extend(
            fields
                .iter()
                .filter(|field| field.owned && domain != pkg)
                .map(|field| format!("{domain}.{}", field.java_type)),
        );
        if fields
            .iter()
            .any(|field| field.optionality == crate::generate::Optionality::Nullable)
        {
            imports.push("java.util.Optional".to_string());
        }
        imports.sort();
        imports.dedup();
        let imports = imports
            .into_iter()
            .map(|import| format!("import {import};\n"))
            .collect::<String>();
        if missing.is_empty() {
            (
                imports,
                String::new(),
                String::new(),
                event_args,
                expected_id,
            )
        } else {
            (
                imports,
                "import org.junit.jupiter.api.Disabled;\n".to_string(),
                format!(
                    "@Disabled(\"todo: supply a sample for {} -- jails cannot build the full event\")\n",
                    missing.join(", ")
                ),
                event_args,
                expected_id,
            )
        }
    };
    crate::template::render(
        include_str!("../templates/spring/messaging_it_java.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("topic", topic),
            ("event_imports", &event_imports),
            ("disabled_import", &disabled_import),
            ("disabled", &disabled),
            ("event_args", &event_args),
            ("expected_id", &expected_id),
        ],
    )
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn typed_events_share_the_field_model_and_keep_a_string_kafka_key() {
        let fields = crate::generate::parse_fields_for_test(&[
            "id:uuid".to_string(),
            "url:uri".to_string(),
            "occurredAt:instant".to_string(),
        ])
        .unwrap();
        let root = Path::new("/tmp/jails-event-field-test");
        let files = event_files(
            root,
            "com.example.messaging",
            "com.example.domain",
            "PageDiscovered",
            &fields,
        )
        .unwrap();

        let event = &files[0].1;
        assert!(event.contains("record PageDiscoveredEvent(UUID id, URI url, Instant occurredAt)"));
        let publisher = &files[1].1;
        assert!(publisher.contains("kafka.send(topic, String.valueOf(event.id()), event)"));
        let integration_test = &files[3].1;
        assert!(
            integration_test.contains("UUID.fromString"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("URI.create"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("Instant.parse"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("isEqualTo(UUID.fromString"),
            "{integration_test}"
        );
    }

    #[test]
    fn typed_events_refuse_to_invent_a_durable_identity() {
        let fields =
            crate::generate::parse_fields_for_test(&["occurredAt:instant".to_string()]).unwrap();
        let error = event_files(
            Path::new("/tmp/jails-event-field-test"),
            "com.example.messaging",
            "com.example.domain",
            "PageDiscovered",
            &fields,
        )
        .unwrap_err();
        assert!(error.contains("stable `id`"), "{error}");
    }
}
// ---------------------------------------------------------------------------
// `add security` -- an explicit filter chain, rather than the default one.
// ---------------------------------------------------------------------------

pub(crate) const SECURITY_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-security",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const SECURITY_TEST: Dependency = Dependency {
    group_id: "org.springframework.security",
    artifact_id: "spring-security-test",
    version: None,
    scope: Some("test"),
    optional: false,
};

pub(crate) const OAUTH2_RESOURCE_SERVER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-oauth2-resource-server",
    version: None,
    scope: None,
    optional: false,
};

/// The security slice.
///
/// Adding `spring-boot-starter-security` on its own changes the application
/// immediately and drastically: every endpoint requires authentication, a
/// login form appears, and a generated password is printed to the log once at
/// startup. That is a safe default and a bewildering one -- the usual first
/// reaction is to search for how to turn it off, which is how applications
/// end up with `permitAll()` on everything.
///
/// So this writes the filter chain explicitly. An explicit chain is readable,
/// reviewable, and testable, and the generated test asserts both directions:
/// anonymous requests are rejected, authenticated ones are not.
pub(crate) fn security_slice(root: &Path, pkg: &str) -> SpringSlice {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![SECURITY_STARTER, OAUTH2_RESOURCE_SERVER, SECURITY_TEST],
        files: vec![
            (main.join("SecurityConfig.java"), security_config_java(pkg)),
            (
                main.join("ProductionSecurityConfig.java"),
                production_security_config_java(pkg),
            ),
            (
                main.join("ScopeAuthorizer.java"),
                scope_authorizer_java(pkg),
            ),
            (
                test.join("SecurityConfigTest.java"),
                security_test_java(pkg),
            ),
            (
                test.join("ScopeAuthorizerTest.java"),
                scope_authorizer_test_java(pkg),
            ),
        ],
        properties: Vec::new(),
    }
}

fn security_config_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn production_security_config_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/production_security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/security_test_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/scope_authorizer_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/scope_authorizer_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// The scaffold's service and controller -- working CRUD rather than stubs.
// ---------------------------------------------------------------------------

/// The application service a scaffolded resource gets.
///
/// Thin on purpose: it delegates to the port and returns domain types. What
/// it buys is a seam -- the controller depends on this rather than on a
/// repository, so the day one of these operations grows a rule (a permission
/// check, an event to publish) there is somewhere for it to go that is not a
/// controller method.
pub(crate) fn resource_service_java(pkg: &str, name: &str, extra: &str) -> String {
    let var = crate::generate::lower_first(name);
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {{@link {name}}}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class {name}Service {{

    private final {name}Repository repository;

    public {name}Service({name}Repository repository) {{
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }}

    public List<{name}> findAll() {{
        return repository.findAll();
    }}

    public Optional<{name}> findById(String id) {{
        return repository.findById(id);
    }}

    public {name} create({name} {var}) {{
        repository.save({var});
        return {var};
    }}

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {{
        return repository.deleteById(id);
    }}
}}
"#
    )
}

/// A REST resource with the four operations that actually exist, wired to
/// the service and speaking in DTOs.
///
/// The status codes are the ones the situations mean, which is most of what
/// distinguishes a REST API from a set of methods reachable over HTTP: 201
/// with a `Location` for a creation, 204 for a delete that removed
/// something, 404 for one that did not, and 404 rather than an empty 200 for
/// a missing item.
pub(crate) fn resource_controller_java(
    security: &str,
    pkg: &str,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    if fields.iter().any(|field| field.constraints.scoped) {
        return scoped_resource_controller_java(security, pkg, name, extra, has_id, fields);
    }
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    // A `Location` header needs something to point at. Without an `id`
    // component there is no per-item URL to build, and inventing one would
    // be worse than omitting the header.
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n\
                 \x20               .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        // No `id` component, so there is no per-item URL to\n\
                 \x20       // advertise in a Location header.\n\
                 \x20       return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    format!(
        r#"package {pkg};

{extra}{location_import}import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
{status_import}import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * HTTP for {{@link {name}}}.
 *
 * <p>Speaks in {{@link {name}Request}} and {{@link {name}Response}} rather
 * than the domain type, so the wire contract and the domain can change
 * independently.
 *
 * <p>{{@code @Valid}} rejects a malformed body before any application code
 * runs. With {{@code jails add api}} the rejection is reported as an RFC 9457
 * problem naming each bad field; without it, Spring's default 400 says only
 * that something was wrong.
 */
@RestController
@RequestMapping({name}Controller.PATH)
public class {name}Controller {{

    /** The collection this controller serves. */
    public static final String PATH = "{path}";

    private final {name}Service service;

    public {name}Controller({name}Service service) {{
        this.service = Objects.requireNonNull(service, "service is required");
    }}

    @GetMapping
    public List<{name}Response> list() {{
        return service.findAll().stream().map({name}Response::from).toList();
    }}

    /** 404 rather than an empty 200: "no such thing" and "here is nothing" differ. */
    @GetMapping("/{{id}}")
    public ResponseEntity<{name}Response> byId(@PathVariable String id) {{
        return service.findById(id)
                .map({name}Response::from)
                .map(ResponseEntity::ok)
                .orElseGet(() -> ResponseEntity.notFound().build());
    }}

    @PostMapping
    public ResponseEntity<{name}Response> create(@Valid @RequestBody {name}Request request) {{
        {name} created = service.create(request.toDomain());
{created}
    }}

    /** 204 when something was removed, 404 when there was nothing to remove. */
    @DeleteMapping("/{{id}}")
    public ResponseEntity<Void> delete(@PathVariable String id) {{
        return service.deleteById(id)
                ? ResponseEntity.noContent().build()
                : ResponseEntity.notFound().build();
    }}
}}
"#
    )
}

fn scoped_resource_controller_java(
    security: &str,
    pkg: &str,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, pkg, fields, "request");
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n                 .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    format!(
        r#"package {pkg};

{extra}{scope_import}{location_import}import jakarta.validation.Valid;
import java.util.Objects;
{status_import}import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * Scope-safe creation endpoint for {{@link {name}}}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {{@code @scope}} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping({name}Controller.PATH)
public class {name}Controller {{

    public static final String PATH = "{path}";

    private final {name}Service service;
{scope_field}

    public {name}Controller({name}Service service{scope_constructor}) {{
        this.service = Objects.requireNonNull(service, "service is required");
{scope_assignment}
    }}

    @PostMapping
    public ResponseEntity<{name}Response> create(
            @Valid @RequestBody {name}Request request{scope_parameter}) {{
{scope_checks}
        {name} created = service.create(request.toDomain());
{created}
    }}
}}
"#
    )
}

/// The controller's test: a web-layer slice with the service replaced.
///
/// `@WebMvcTest` starts the web layer and nothing else -- no database, no
/// component scan of the whole application -- so it runs in a fraction of
/// the time a `@SpringBootTest` takes and fails for reasons that are about
/// HTTP. The service is a `@MockitoBean`, which is the current spelling:
/// `@MockBean` no longer exists in Spring Boot 4.
pub(crate) fn resource_controller_test_java(
    security: &str,
    pkg: &str,
    name: &str,
    extra: &str,
    fields: &[crate::generate::Field],
    webmvc_test_import: &str,
) -> String {
    if fields.iter().any(|field| field.constraints.scoped) {
        let guard_import = crate::generate::import_of(pkg, security, "ScopeAuthorizer");
        return format!(
            r#"package {pkg};

{extra}{guard_import}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {webmvc_test_import};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({name}Controller.class)
class {name}ControllerTest {{

    @Autowired private MockMvcTester mvc;
    @MockitoBean private {name}Service service;
    @MockitoBean private ScopeAuthorizer scopeAuthorizer;

    @Test
    void broadUnscopedReadsAreNotExposed() {{
        assertThat(mvc.get().uri({name}Controller.PATH)).hasStatus(405);
        assertThat(mvc.get().uri({name}Controller.PATH + "/other-tenant-id")).hasStatus(404);
    }}
}}
"#
        );
    }
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {webmvc_test_import};

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;

@WebMvcTest({name}Controller.class)
class {name}ControllerTest {{

    @Autowired
    private MockMvcTester mvc;

    @MockitoBean
    private {name}Service service;

    @Test
    void anEmptyCollectionIsAnEmptyArray() {{
        given(service.findAll()).willReturn(List.of());

        assertThat(mvc.get().uri({name}Controller.PATH))
                .hasStatusOk()
                .bodyJson()
                .isEqualTo("[]");
    }}

    @Test
    void aMissingItemIs404() {{
        given(service.findById("nope")).willReturn(Optional.empty());

        assertThat(mvc.get().uri({name}Controller.PATH + "/nope")).hasStatus(404);
    }}

    @Test
    void aDeleteThatRemovedNothingIs404() {{
        given(service.deleteById("nope")).willReturn(false);

        assertThat(mvc.delete().uri({name}Controller.PATH + "/nope")).hasStatus(404);
    }}
}}
"#
    )
}

/// The scaffolded service's test.
///
/// The repository is a Mockito mock rather than a hand-written fake, for one
/// reason: a fake has to key items by something, and jails cannot know which
/// component of an arbitrary record is its identity. A mock needs no such
/// knowledge, so this test compiles for every field spec.
///
/// What it pins is delegation and the two boolean-ish outcomes that are easy
/// to get backwards -- an absent item is `Optional.empty()`, and a delete
/// reports whether anything was actually removed.
pub(crate) fn resource_service_test_java(pkg: &str, name: &str, extra: &str) -> String {
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

class {name}ServiceTest {{

    private final {name}Repository repository = mock({name}Repository.class);
    private final {name}Service service = new {name}Service(repository);

    @Test
    void findAllDelegatesToThePort() {{
        given(repository.findAll()).willReturn(List.of());

        assertThat(service.findAll()).isEmpty();
    }}

    @Test
    void aMissingIdIsEmptyRatherThanNull() {{
        given(repository.findById("nope")).willReturn(Optional.empty());

        assertThat(service.findById("nope")).isEmpty();
    }}

    @Test
    void deleteReportsWhetherAnythingWasRemoved() {{
        given(repository.deleteById("gone")).willReturn(true);
        given(repository.deleteById("never-existed")).willReturn(false);

        assertThat(service.deleteById("gone")).isTrue();
        assertThat(service.deleteById("never-existed")).isFalse();
    }}
}}
"#
    )
}

/// An in-memory adapter, so a freshly scaffolded application starts and
/// serves requests before anyone has wired a database.
///
/// This is the piece that makes `jails g scaffold` produce something you can
/// actually run: the JDBC adapter is deliberately not a bean (it takes a
/// `Connection`, which the caller owns), so without this the context fails to
/// start with "no qualifying bean of type ...Repository" -- a scaffold that
/// compiles and cannot run.
///
/// It is also the honest default for the stage a scaffold is generated at.
/// Swap the `@Component` annotation onto the JDBC adapter when there is a
/// real `DataSource`; keeping both annotated would make two beans qualify for
/// one injection point, which Spring refuses to choose between (`jails
/// beans` reports exactly that).
/// The in-memory adapter.
///
/// `is_bean` decides whether it carries `@Component`, and exactly one of
/// this and the JDBC adapter may -- see `generate::RepositoryWiring`. Two
/// annotated adapters make two beans qualify for one injection point, and the
/// scaffold then compiles and refuses to start.
pub(crate) fn in_memory_repository_java(
    pkg: &str,
    name: &str,
    extra: &str,
    id_accessor: Option<&str>,
    is_bean: bool,
) -> String {
    let var = crate::generate::lower_first(name);
    let (find_by_id, delete_by_id, save_body, note) = match id_accessor {
        Some(accessor) => (
            format!("        return Optional.ofNullable(items.get(id));"),
            format!("        return items.remove(id) != null;"),
            format!("        items.put(String.valueOf({var}.{accessor}()), {var});"),
            " * <p>Keyed on the record's own {@code id} component.\n",
        ),
        None => (
            "        // TODO: this type has no `id` component, so jails cannot\n\
             \x20       // tell which part of it is the identity. Pick one and key\n\
             \x20       // `items` on it.\n\
             \x20       return Optional.empty();"
                .to_string(),
            "        return items.remove(id) != null;".to_string(),
            format!("        items.put(String.valueOf(items.size()), {var});"),
            " * <p>This type declares no {@code id} component, so lookups by id are\n\
             \x20* left unimplemented -- see the TODO in {@code findById}.\n",
        ),
    };
    // Exactly one adapter is the bean. When the JDBC one is, this is a fake
    // for tests and says so rather than pretending to be a stand-in for a
    // database that now exists.
    let repository_annotation = if is_bean { "@Component\n" } else { "" };
    let repository_import = if is_bean {
        "import org.springframework.stereotype.Component;\n"
    } else {
        ""
    };
    let role_note = if is_bean {
        " * <p>When a real {@code DataSource} arrives, `jails add db` makes\n * {@code Jdbc"
            .to_string()
            + name
            + "Repository} the bean and drops the annotation here. Annotating\n * both makes two beans qualify for one injection point, which Spring\n * refuses to choose between.\n"
    } else {
        " * <p>Not a bean: this project has a {@code DataSource}, so {@code Jdbc".to_string()
            + name
            + "Repository}\n * is the {@code @Component}. This stays as a fake for tests that want a\n * repository without a container -- construct it directly.\n"
    };
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
{repository_import}
/**
 * {{@link {name}Repository}} in memory, so the application runs before it has
 * a database.
 *
{note} *
 * <p>{{@link ConcurrentHashMap}} rather than {{@link java.util.HashMap}}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
{role_note} */
{repository_annotation}public class InMemory{name}Repository implements {name}Repository {{

    private final Map<String, {name}> items = new ConcurrentHashMap<>();

    @Override
    public Optional<{name}> findById(String id) {{
{find_by_id}
    }}

    @Override
    public List<{name}> findAll() {{
        return List.copyOf(items.values());
    }}

    @Override
    public void save({name} {var}) {{
{save_body}
    }}

    @Override
    public boolean deleteById(String id) {{
{delete_by_id}
    }}
}}
"#
    )
}

/// The Failsafe plugin, without which every generated `*IT` is dead code.
///
/// Surefire runs `*Test`; `*IT` belongs to Failsafe, and Failsafe is *not*
/// part of the Spring Boot parent's default build. So a project that has
/// never added it runs `mvn verify` to completion, reports success, and
/// executes none of the integration tests -- which is worse than not having
/// them, because the green build says they passed.
///
/// `integration-test` and `verify` are both bound: the first runs the tests,
/// the second is what makes a failure fail the build. Binding only the first
/// runs them and ignores the result.
pub(crate) const FAILSAFE_ARTIFACT: &str = "maven-failsafe-plugin";

pub(crate) const FAILSAFE_PLUGIN: &str = r#"<plugin>
    <groupId>org.apache.maven.plugins</groupId>
    <artifactId>maven-failsafe-plugin</artifactId>
    <executions>
        <execution>
            <goals>
                <!-- `verify` as well as `integration-test`: without it the
                     tests run and their failures are ignored. -->
                <goal>integration-test</goal>
                <goal>verify</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

// ---------------------------------------------------------------------------
// `add redis` -- a key/value store with a compose service and a real test.
// ---------------------------------------------------------------------------

pub(crate) const REDIS_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-data-redis",
    version: None,
    scope: None,
    optional: false,
};

/// Testcontainers' generic container, which is what Boot's Redis
/// `@ServiceConnection` factory matches on: it accepts any
/// `GenericContainer` whose image is one of the Redis images, rather than a
/// dedicated Redis container type.
pub(crate) const TESTCONTAINERS_CORE: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};

pub(crate) const REDIS_IMAGE: &str = "redis:7-alpine";

pub(crate) fn redis_slice(root: &Path, pkg: &str) -> SpringSlice {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![REDIS_STARTER, TESTCONTAINERS_CORE, SPRING_TESTCONTAINERS],
        files: vec![
            (main.join("KeyValueStore.java"), key_value_store_java(pkg)),
            (
                test.join("KeyValueStoreIT.java"),
                key_value_store_it_java(pkg),
            ),
        ],
        properties: vec![
            "spring.data.redis.host=localhost".to_string(),
            "spring.data.redis.port=6379".to_string(),
            // A key/value store is a cache, not a database, and a cache
            // without expiry is a memory leak that survives restarts. This is
            // the default the wrapper applies when no TTL is given.
            "app.redis.default-ttl=PT10M".to_string(),
        ],
    }
}

fn key_value_store_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/key_value_store_java.java"),
        &[("pkg", pkg)],
    )
}

fn key_value_store_it_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/key_value_store_it_java.java"),
        &[("pkg", pkg), ("REDIS_IMAGE", REDIS_IMAGE)],
    )
}

// ---------------------------------------------------------------------------
// `add observability` -- metrics that are attributable and scrapeable.
// ---------------------------------------------------------------------------

/// Version managed by the Spring Boot parent, which imports the Micrometer
/// BOM (verified in spring-boot-dependencies, not assumed).
pub(crate) const PROMETHEUS_REGISTRY: Dependency = Dependency {
    group_id: "io.micrometer",
    artifact_id: "micrometer-registry-prometheus",
    version: None,
    scope: None,
    optional: false,
};

/// Metrics, exposed for scraping and tagged so they can be told apart.
///
/// The boilerplate this removes is not the dependency -- it is the two
/// conventions people rediscover per project. Metrics with no common tag are
/// unattributable the moment a second service reports to the same Prometheus,
/// and a counter created inline per call site gets a slightly different name
/// each time (`orders.created`, `order_created`, `ordersCreated`) until the
/// dashboards stop agreeing.
pub(crate) fn observability_slice(root: &Path, pkg: &str) -> SpringSlice {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![ACTUATOR_STARTER, PROMETHEUS_REGISTRY],
        files: vec![
            (
                main.join("MetricsConfig.java"),
                metrics_config_java(pkg, meter_registry_customizer_import(root)),
            ),
            (main.join("AppMetrics.java"), app_metrics_java(pkg)),
            (test.join("AppMetricsTest.java"), app_metrics_test_java(pkg)),
            (
                test.join("PrometheusScrapeTest.java"),
                prometheus_scrape_test_java(
                    pkg,
                    crate::generate::mockmvc_autoconfigure_import(root),
                ),
            ),
        ],
        properties: vec![
            // `prometheus` in addition to the actuator defaults. Still named
            // individually rather than `*`, which would publish heapdump and
            // the resolved environment.
            exposure_include(root, &["health", "info", "metrics", "prometheus"]),
        ],
    }
}

fn app_metrics_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/app_metrics_java.java"),
        &[("pkg", pkg)],
    )
}

fn app_metrics_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/app_metrics_test_java.java"),
        &[("pkg", pkg)],
    )
}

#[cfg(test)]
mod observability_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-observability-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture(tag: &str, properties: &str) -> std::path::PathBuf {
        let dir = scratch(tag);
        let resources = dir.join("src/main/resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("application.properties"), properties).unwrap();
        dir
    }

    #[test]
    fn the_exposure_list_unions_rather_than_replaces() {
        let dir = fixture(
            "union",
            "management.endpoints.web.exposure.include=health,info,metrics\n",
        );
        assert_eq!(
            exposure_include(&dir, &["health", "info", "metrics", "prometheus"]),
            "management.endpoints.web.exposure.include=health,info,metrics,prometheus"
        );
    }

    #[test]
    fn a_narrower_capability_does_not_drop_what_a_wider_one_exposed() {
        // `add observability` then `add actuator`: actuator's own list is a
        // subset, and appending it verbatim would win and hide the scrape.
        let dir = fixture(
            "narrower",
            "management.endpoints.web.exposure.include=health,info,metrics,prometheus\n",
        );
        assert!(exposure_include(&dir, &["health", "info", "metrics"]).ends_with("prometheus"));
    }

    #[test]
    fn a_hand_widened_list_is_preserved_not_rewritten() {
        let dir = fixture(
            "hand-widened",
            "management.endpoints.web.exposure.include=health,loggers\n",
        );
        let line = exposure_include(&dir, &["health", "info", "metrics"]);
        assert!(line.contains("loggers"), "{line}");
    }

    #[test]
    fn no_properties_file_yields_just_the_wanted_names() {
        let dir = scratch("absent");
        assert_eq!(
            exposure_include(&dir, &["health", "prometheus"]),
            "management.endpoints.web.exposure.include=health,prometheus"
        );
    }
}

fn prometheus_scrape_test_java(pkg: &str, mockmvc_import: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/prometheus_scrape_test_java.java"),
        &[("pkg", pkg), ("mockmvc_import", mockmvc_import)],
    )
}

/// Boot 4 moved `MeterRegistryCustomizer` out of `actuate.autoconfigure`, with
/// no shim -- the same class of break as `@AutoConfigureMockMvc`.
fn meter_registry_customizer_import(root: &Path) -> &'static str {
    const LEGACY: &str =
        "org.springframework.boot.actuate.autoconfigure.metrics.MeterRegistryCustomizer";
    const CURRENT: &str =
        "org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer";
    if crate::generate::spring_boot_major(root) >= 4 {
        CURRENT
    } else {
        LEGACY
    }
}

fn metrics_config_java(pkg: &str, customizer_import: &str) -> String {
    crate::template::render(
        include_str!("../templates/spring/metrics_config_java.java"),
        &[("pkg", pkg), ("customizer_import", customizer_import)],
    )
}
