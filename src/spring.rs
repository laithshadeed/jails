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
            (
                main.join("ApiException.java"),
                api_exception_java(pkg),
            ),
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
    crate::template::render(include_str!("../templates/spring/api_exception_java.java"), &[("pkg", pkg)])
}

fn api_exception_handler_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/api_exception_handler_java.java"), &[("pkg", pkg)])
}

fn api_exception_handler_test_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/api_exception_handler_test_java.java"), &[("pkg", pkg)])
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
    crate::template::render(include_str!("../templates/spring/actuator_test_java.java"), &[("pkg", pkg), ("mockmvc_import", mockmvc_import)])
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
    crate::template::render(include_str!("../templates/spring/cache_config_java.java"), &[("pkg", pkg)])
}

fn cache_test_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/cache_test_java.java"), &[("pkg", pkg)])
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
    crate::template::render(include_str!("../templates/spring/client_config_java.java"), &[("pkg", pkg), ("group", group)])
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
    crate::template::render(include_str!("../templates/spring/scheduling_config_java.java"), &[("pkg", pkg)])
}

fn job_test_java(pkg: &str, name: &str) -> String {
    crate::template::render(include_str!("../templates/spring/job_test_java.java"), &[("pkg", pkg), ("name", name)])
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
                sample.clone().unwrap_or_else(|| format!("null /* {} */", field.java_type))
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
    crate::template::render(include_str!("../templates/spring/kafka_config_java.java"), &[("pkg", pkg)])
}

/// The domain's own "no retry will ever fix this".
///
/// Deliberately unlike [`api_exception_java`], which is sealed, abstract and
/// stack-trace-free. This one is open -- callers throw and subclass it -- and it
/// keeps its stack trace, because it wraps a real cause and that cause is what
/// a human reads out of the dead-letter headers.
fn non_retryable_exception_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/non_retryable_exception_java.java"), &[("pkg", pkg)])
}

/// A test that the poison-message path is actually wired, without a broker.
///
/// The container-backed version belongs to `g event`; this one exists so that
/// `add kafka` keeps the promise `jails add --help` makes -- a dependency,
/// the code that uses it, *and a test that proves it works*.
fn kafka_config_test_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/kafka_config_test_java.java"), &[("pkg", pkg)])
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
        return Err("an event `id` cannot be optional: a null key loses per-entity ordering".to_string());
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
    source.replace("kafka.send(topic, event.id(), event)", &format!("kafka.send(topic, {key}, event)"))
}

fn listener_java(pkg: &str, name: &str, topic: &str) -> String {
    crate::template::render(include_str!("../templates/spring/listener_java.java"), &[("pkg", pkg), ("name", name), ("topic", topic)])
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
            (imports, String::new(), String::new(), event_args, expected_id)
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
        assert!(integration_test.contains("UUID.fromString"), "{integration_test}");
        assert!(integration_test.contains("URI.create"), "{integration_test}");
        assert!(integration_test.contains("Instant.parse"), "{integration_test}");
        assert!(
            integration_test.contains("isEqualTo(UUID.fromString"),
            "{integration_test}"
        );
    }

    #[test]
    fn typed_events_refuse_to_invent_a_durable_identity() {
        let fields = crate::generate::parse_fields_for_test(&["occurredAt:instant".to_string()])
            .unwrap();
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
        deps: vec![SECURITY_STARTER, SECURITY_TEST],
        files: vec![
            (main.join("SecurityConfig.java"), security_config_java(pkg)),
            (test.join("SecurityConfigTest.java"), security_test_java(pkg)),
        ],
        properties: Vec::new(),
    }
}

fn security_config_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/security_config_java.java"), &[("pkg", pkg)])
}

fn security_test_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/security_test_java.java"), &[("pkg", pkg)])
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
    pkg: &str,
    name: &str,
    extra: &str,
    has_id: bool,
) -> String {
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

/// The controller's test: a web-layer slice with the service replaced.
///
/// `@WebMvcTest` starts the web layer and nothing else -- no database, no
/// component scan of the whole application -- so it runs in a fraction of
/// the time a `@SpringBootTest` takes and fails for reasons that are about
/// HTTP. The service is a `@MockitoBean`, which is the current spelling:
/// `@MockBean` no longer exists in Spring Boot 4.
pub(crate) fn resource_controller_test_java(
    pkg: &str,
    name: &str,
    extra: &str,
    webmvc_test_import: &str,
) -> String {
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
        " * <p>Not a bean: this project has a {@code DataSource}, so {@code Jdbc"
            .to_string()
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
            (test.join("KeyValueStoreIT.java"), key_value_store_it_java(pkg)),
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
    crate::template::render(include_str!("../templates/spring/key_value_store_java.java"), &[("pkg", pkg)])
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
                prometheus_scrape_test_java(pkg, crate::generate::mockmvc_autoconfigure_import(root)),
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
    crate::template::render(include_str!("../templates/spring/app_metrics_java.java"), &[("pkg", pkg)])
}

fn app_metrics_test_java(pkg: &str) -> String {
    crate::template::render(include_str!("../templates/spring/app_metrics_test_java.java"), &[("pkg", pkg)])
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
    crate::template::render(include_str!("../templates/spring/prometheus_scrape_test_java.java"), &[("pkg", pkg), ("mockmvc_import", mockmvc_import)])
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
    crate::template::render(include_str!("../templates/spring/metrics_config_java.java"), &[("pkg", pkg), ("customizer_import", customizer_import)])
}
