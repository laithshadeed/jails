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
    format!(
        r#"package {pkg};

/**
 * A failure this application knows how to describe to a client.
 *
 * <p>Sealed on purpose. {{@link ApiExceptionHandler}} switches over these to
 * choose a status code, and that switch has no {{@code default}} branch -- so
 * adding a variant here stops the build until someone decides what it means
 * over HTTP. An open hierarchy would instead let a new failure quietly become
 * a 500.
 *
 * <p>Abstract as well as sealed: a sealed class that can itself be
 * instantiated is one more case the switch has to cover, and javac says so.
 *
 * <p>These carry no stack trace: they describe an expected outcome (the id was
 * not there, the version had moved on), not a bug, and collecting a trace for
 * every 404 is pure cost.
 */
public abstract sealed class ApiException extends RuntimeException {{

    private ApiException(String message) {{
        // No writable stack trace, no suppression: an expected outcome does
        // not need the cost of a fill-in.
        super(message, null, false, false);
    }}

    /** Nothing with that identity exists. Becomes a 404. */
    public static final class NotFound extends ApiException {{
        public NotFound(String message) {{
            super(message);
        }}
    }}

    /** The request conflicts with the current state. Becomes a 409. */
    public static final class Conflict extends ApiException {{
        public Conflict(String message) {{
            super(message);
        }}
    }}

    /**
     * The request was well-formed but the domain rejected it. Becomes a 422 --
     * as opposed to a 400, which means jails could not read the request at all.
     */
    public static final class Rejected extends ApiException {{
        public Rejected(String message) {{
            super(message);
        }}
    }}
}}
"#
    )
}

fn api_exception_handler_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.LinkedHashMap;
import java.util.Map;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.HttpStatusCode;
import org.springframework.http.ProblemDetail;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.MethodArgumentNotValidException;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;
import org.springframework.web.context.request.WebRequest;
import org.springframework.web.servlet.mvc.method.annotation.ResponseEntityExceptionHandler;

/**
 * Turns failures into RFC 9457 problem responses, in one place.
 *
 * <p>Extends Spring's own {{@link ResponseEntityExceptionHandler}} rather than
 * starting from nothing, so every exception the framework already understands
 * -- an unreadable body, a missing parameter, an unsupported media type --
 * keeps the status code Spring chose for it. Only this application's own
 * failures need a mapping, and they are the sealed set in
 * {{@link ApiException}}.
 *
 * <p>The response body is {{@code application/problem+json}}: a media type
 * with a specification behind it, rather than a {{@code Map<String, String>}}
 * shaped differently in each controller.
 */
@RestControllerAdvice
public class ApiExceptionHandler extends ResponseEntityExceptionHandler {{

    /**
     * The application's own failures. The switch has no {{@code default}}:
     * a new {{@link ApiException}} variant breaks this build until its status
     * is decided here.
     */
    @ExceptionHandler(ApiException.class)
    public ProblemDetail handleApiException(ApiException failure) {{
        HttpStatus status =
                switch (failure) {{
                    case ApiException.NotFound ignored -> HttpStatus.NOT_FOUND;
                    case ApiException.Conflict ignored -> HttpStatus.CONFLICT;
                    case ApiException.Rejected ignored -> HttpStatus.UNPROCESSABLE_ENTITY;
                }};
        return ProblemDetail.forStatusAndDetail(status, failure.getMessage());
    }}

    /**
     * Bean-validation failures on a request body or parameter.
     *
     * <p>Spring's default renders these as a 400 with no indication of which
     * field was wrong, which is the single most common reason a client
     * integration stalls. The field errors go into a {{@code fields}} extension
     * member -- an RFC 9457 problem document is explicitly extensible, so this
     * needs no bespoke error envelope.
     */
    @Override
    protected ResponseEntity<Object> handleMethodArgumentNotValid(
            MethodArgumentNotValidException failure,
            HttpHeaders headers,
            HttpStatusCode status,
            WebRequest request) {{
        ProblemDetail problem =
                ProblemDetail.forStatusAndDetail(status, "the request has invalid fields");
        // LinkedHashMap: field order follows declaration order, so the
        // response is stable and diffable between runs.
        Map<String, String> fields = new LinkedHashMap<>();
        failure.getBindingResult()
                .getFieldErrors()
                .forEach(error -> fields.putIfAbsent(error.getField(), message(error.getDefaultMessage())));
        problem.setProperty("fields", fields);
        return handleExceptionInternal(failure, problem, headers, status, request);
    }}

    private static String message(String defaultMessage) {{
        return defaultMessage == null ? "is invalid" : defaultMessage;
    }}
}}
"#
    )
}

fn api_exception_handler_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpStatus;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import org.springframework.test.web.servlet.setup.StandaloneMockMvcBuilder;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Drives the advice through a standalone MockMvc rather than a
 * {{@code @SpringBootTest}}: no application context, no database, no port, so
 * it runs in milliseconds and keeps failing for exactly one reason.
 *
 * <p>The controller below exists only to throw. Testing the advice against
 * the real controllers would couple this test to whatever they happen to do
 * today.
 */
class ApiExceptionHandlerTest {{

    private final MockMvcTester mvc =
            MockMvcTester.of(
                    List.of(new ThrowingController()),
                    builder -> builder.setControllerAdvice(new ApiExceptionHandler()).build());

    @Test
    void aMissingThingBecomesA404Problem() {{
        assertThat(mvc.get().uri("/boom/not-found"))
                .hasStatus(HttpStatus.NOT_FOUND)
                .bodyJson()
                .extractingPath("$.detail")
                .isEqualTo("no such thing");
    }}

    @Test
    void aConflictBecomesA409() {{
        assertThat(mvc.get().uri("/boom/conflict")).hasStatus(HttpStatus.CONFLICT);
    }}

    @Test
    void aDomainRejectionBecomesA422() {{
        // 422, not 400: the request was read successfully and the domain said
        // no. A 400 would tell the client to fix its syntax.
        assertThat(mvc.get().uri("/boom/rejected"))
                .hasStatus(HttpStatus.UNPROCESSABLE_ENTITY);
    }}

    @RestController
    static class ThrowingController {{

        @GetMapping("/boom/not-found")
        String notFound() {{
            throw new ApiException.NotFound("no such thing");
        }}

        @GetMapping("/boom/conflict")
        String conflict() {{
            throw new ApiException.Conflict("already exists");
        }}

        @GetMapping("/boom/rejected")
        String rejected() {{
            throw new ApiException.Rejected("amount must be positive");
        }}
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// `add actuator` -- health, metrics and info, without inventing endpoints.
// ---------------------------------------------------------------------------

pub(crate) fn actuator_slice(root: &Path, pkg: &str) -> SpringSlice {
    let test = crate::generate::test_dir(root, pkg);
    SpringSlice {
        deps: vec![ACTUATOR_STARTER],
        files: vec![(
            test.join("ActuatorEndpointsTest.java"),
            actuator_test_java(pkg),
        )],
        // Exposed deliberately and narrowly. The default over HTTP is health
        // alone; `*` is the shape that leaks heap dumps and environment
        // variables to anything that can reach the port.
        properties: vec![
            "management.endpoints.web.exposure.include=health,info,metrics".to_string(),
            "management.endpoint.health.show-details=when-authorized".to_string(),
        ],
    }
}

fn actuator_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Pins the endpoints that are exposed, both ways.
 *
 * <p>The second assertion is the one that earns its place: `management.
 * endpoints.web.exposure.include` is a list people widen to `*` under time
 * pressure, and `*` publishes heap dumps and the resolved environment --
 * credentials included -- to anything that can reach the port. A test that
 * fails when that happens is cheaper than noticing in production.
 */
@SpringBootTest
@AutoConfigureMockMvc
class ActuatorEndpointsTest {{

    @Autowired
    private MockMvcTester mvc;

    @Test
    void healthIsExposed() {{
        assertThat(mvc.get().uri("/actuator/health")).hasStatusOk();
    }}

    @Test
    void everythingElseStaysUnexposed() {{
        // 4xx rather than 404 specifically: an unexposed endpoint is a 404,
        // but once `jails add security` is in the project it becomes a 401
        // instead. Both mean "not available"; pinning 404 would make this
        // test fail the day the application is secured, which is exactly
        // backwards.
        assertThat(mvc.get().uri("/actuator/env")).hasStatus4xxClientError();
        assertThat(mvc.get().uri("/actuator/heapdump")).hasStatus4xxClientError();
    }}
}}
"#
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
    format!(
        r#"package {pkg};

import org.springframework.cache.annotation.EnableCaching;
import org.springframework.context.annotation.Configuration;

/**
 * Turns on {{@code @Cacheable}} and friends.
 *
 * <p>Spring Boot auto-configures a {{@code CacheManager}} from
 * {{@code spring.cache.*}}, but caching itself stays off until something
 * enables it -- which is why a freshly added {{@code @Cacheable}} so often
 * appears to do nothing at all.
 *
 * <p>The bound in {{@code spring.cache.caffeine.spec}} is not decoration: an
 * unbounded cache is a memory leak that reports itself as a performance
 * feature.
 */
@Configuration(proxyBeanMethods = false)
@EnableCaching
public class CacheConfig {{}}
"#
    )
}

fn cache_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.cache.annotation.Cacheable;
import org.springframework.context.annotation.Bean;
import org.springframework.stereotype.Component;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Proves caching is actually switched on.
 *
 * <p>Worth a test because the failure is silent: without {{@code @EnableCaching}}
 * a {{@code @Cacheable}} method simply runs every time, and nothing anywhere
 * reports a problem. Counting invocations is the only way to tell the two
 * states apart.
 */
@SpringBootTest
class CacheConfigTest {{

    @Autowired
    private Counter counter;

    @Test
    void aSecondCallWithTheSameArgumentDoesNotRunTheMethod() {{
        counter.reset();

        assertThat(counter.slow("a")).isEqualTo(1);
        assertThat(counter.slow("a")).isEqualTo(1);
        assertThat(counter.calls()).isEqualTo(1);

        // A different argument is a different cache key, so the method runs.
        assertThat(counter.slow("b")).isEqualTo(2);
        assertThat(counter.calls()).isEqualTo(2);
    }}

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {{

        @Bean
        Counter counter() {{
            return new Counter();
        }}
    }}

    /** Self-proxied through Spring, so {{@code @Cacheable}} actually applies. */
    @Component
    static class Counter {{

        private final AtomicInteger calls = new AtomicInteger();

        @Cacheable("jails-cache-probe")
        public int slow(String key) {{
            return calls.incrementAndGet();
        }}

        int calls() {{
            return calls.get();
        }}

        void reset() {{
            calls.set(0);
        }}
    }}
}}
"#
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
    format!(
        r#"package {pkg};

import org.springframework.context.annotation.Configuration;
import org.springframework.web.service.registry.ImportHttpServices;

/**
 * Registers this package's {{@code @HttpExchange}} interfaces as beans.
 *
 * <p>Scanned by package rather than listed by type, so a new client interface
 * dropped in here is wired up with no edit to this file.
 *
 * <p>The group name is what links the clients to their configuration:
 * {{@code spring.http.serviceclient.{group}.base-url}} sets where they point,
 * and the same prefix carries timeouts, default headers and SSL bundles.
 */
@Configuration(proxyBeanMethods = false)
@ImportHttpServices(group = "{group}", basePackages = "{pkg}")
public class HttpClientsConfig {{}}
"#
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
    format!(
        r#"package {pkg};

import org.springframework.context.annotation.Configuration;
import org.springframework.scheduling.annotation.EnableScheduling;

/**
 * Turns on {{@code @Scheduled}}.
 *
 * <p>Without this, every {{@code @Scheduled}} method in the application is
 * inert and nothing says so -- the same silent-no-op failure mode as
 * {{@code @EnableCaching}}.
 */
@Configuration(proxyBeanMethods = false)
@EnableScheduling
public class SchedulingConfig {{}}
"#
    )
}

fn job_test_java(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThatCode;

/**
 * Calls the work directly rather than waiting for a schedule.
 *
 * <p>A test that sleeps until the scheduler fires is slow and flaky, and it
 * tests Spring's scheduler rather than this job. What is worth asserting here
 * is that {{@code run()}} does not propagate -- because an exception escaping a
 * scheduled method cancels every future run.
 */
class {name}JobTest {{

    private final {name}Job job = new {name}Job();

    @Test
    void theWorkRuns() {{
        assertThatCode(job::work).doesNotThrowAnyException();
    }}

    @Test
    void aFailureNeverEscapesAndCancelsTheSchedule() {{
        {name}Job failing =
                new {name}Job() {{
                    @Override
                    void work() {{
                        throw new IllegalStateException("boom");
                    }}
                }};
        assertThatCode(failing::run).doesNotThrowAnyException();
    }}
}}
"#
    )
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
            request_java(pkg, name, fields, &domain_import),
            "request",
        ),
        (
            main.join(format!("{name}Response.java")),
            response_java(pkg, name, fields, &domain_import),
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
    field
        .java_type
        .strip_prefix("Optional<")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(&field.java_type)
        .to_string()
}

fn dto_imports(fields: &[crate::generate::Field], with_validation: bool) -> String {
    let mut imports: Vec<String> = Vec::new();
    for field in fields {
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
    if field.java_type.starts_with("Optional<") {
        format!("{accessor}.orElse(null)")
    } else {
        accessor
    }
}

/// The reverse: a nullable wire component becomes an `Optional` again. The
/// generated record's compact constructor normalises a null Optional, so
/// `ofNullable` is enough.
fn write_to_domain(field: &crate::generate::Field) -> String {
    if field.java_type.starts_with("Optional<") {
        format!("Optional.ofNullable({})", field.name)
    } else {
        field.name.clone()
    }
}

fn needs_optional(fields: &[crate::generate::Field]) -> bool {
    fields.iter().any(|f| f.java_type.starts_with("Optional<"))
}

fn request_java(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
) -> String {
    let imports = dto_imports(fields, true);
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

fn response_java(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
) -> String {
    let imports = dto_imports(fields, false);
    let components = components(fields, false);
    let arguments = fields
        .iter()
        .map(|field| read_from_domain(field, &name.to_lowercase()))
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = name.to_lowercase();
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
    let var = name.to_lowercase();
    // A request component is the *wire* type: an Optional domain component is
    // a plain nullable field here, so `Optional.empty()` would not compile as
    // its sample. `null` is the honest wire-level equivalent.
    let samples: Vec<Option<String>> = fields
        .iter()
        .map(|field| {
            if field.java_type.starts_with("Optional<") {
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
    let sample_imports = dto_imports(fields, false);

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
        "spring.kafka.consumer.value-deserializer=org.springframework.kafka.support.serializer.JacksonJsonDeserializer".to_string(),
        format!("spring.kafka.consumer.properties.spring.json.trusted.packages={base}"),
    ]
}

pub(crate) fn event_files(
    root: &Path,
    pkg: &str,
    name: &str,
) -> Vec<(std::path::PathBuf, String, &'static str)> {
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let topic = crate::sql::snake_case(name).replace('_', "-");
    vec![
        (
            main.join(format!("{name}Event.java")),
            event_java(pkg, name),
            "event",
        ),
        (
            main.join(format!("{name}Publisher.java")),
            publisher_java(pkg, name, &topic),
            "publisher",
        ),
        (
            main.join(format!("{name}Listener.java")),
            listener_java(pkg, name, &topic),
            "listener",
        ),
        (
            test.join(format!("{name}MessagingIT.java")),
            messaging_it_java(pkg, name, &topic),
            "messaging integration test",
        ),
    ]
}

fn event_java(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import java.time.Instant;

/**
 * What crosses the topic.
 *
 * <p>A record of its own, not a domain type. A message is a published
 * contract that outlives the process that sent it -- consumers read messages
 * written by older versions -- so it needs to change on its own schedule.
 * Reusing the domain type couples every consumer to an internal refactor.
 *
 * <p>{{@code occurredAt}} is on the event rather than inferred from the
 * broker: the time something happened and the time it was published are
 * different facts, and only the first one survives a replay.
 */
public record {name}Event(String id, Instant occurredAt) {{}}
"#
    )
}

fn publisher_java(pkg: &str, name: &str, topic: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.stereotype.Component;

/**
 * Publishes {{@link {name}Event}}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
 * <p>The key is the event id, which is what gives ordering per entity --
 * Kafka only guarantees order within a partition, and a null key round-robins
 * across all of them. Getting this wrong produces a system that works until
 * it has traffic.
 */
@Component
public class {name}Publisher {{

    private final KafkaTemplate<String, {name}Event> kafka;
    private final String topic;

    public {name}Publisher(
            KafkaTemplate<String, {name}Event> kafka,
            @org.springframework.beans.factory.annotation.Value("${{topics.{topic}:{topic}}}") String topic) {{
        this.kafka = kafka;
        this.topic = topic;
    }}

    /** Publishes asynchronously; the send is in flight when this returns. */
    public void publish({name}Event event) {{
        kafka.send(topic, event.id(), event);
    }}
}}
"#
    )
}

fn listener_java(pkg: &str, name: &str, topic: &str) -> String {
    format!(
        r#"package {pkg};

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.stereotype.Component;

/**
 * Consumes {{@link {name}Event}}.
 *
 * <p>The listener is deliberately thin: it hands the event to the application
 * and does nothing else. Business logic inside a listener is unreachable from
 * any test that does not start a broker, and unreusable from any other entry
 * point.
 *
 * <p>Nothing here catches exceptions. That is the right default -- a thrown
 * exception means the offset is not committed, so the message is retried and
 * eventually goes to a dead-letter topic if one is configured. Swallowing it
 * would acknowledge a message that was never processed, which is data loss
 * that looks like success.
 */
@Component
public class {name}Listener {{

    private static final Logger log = LoggerFactory.getLogger({name}Listener.class);

    @KafkaListener(topics = "${{topics.{topic}:{topic}}}")
    public void on({name}Event event) {{
        log.info("received {{}}", event.id());
        // TODO: hand this to the application service that owns the reaction.
    }}
}}
"#
    )
}

fn messaging_it_java(pkg: &str, name: &str, topic: &str) -> String {
    format!(
        r#"package {pkg};

import java.time.Duration;
import java.time.Instant;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.kafka.annotation.KafkaListener;
import org.testcontainers.kafka.KafkaContainer;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Publishes through a real broker and waits for it to come back.
 *
 * <p>An {{@code IT}}, so Failsafe runs it in {{@code verify}} rather than on
 * every {{@code jails test}}: starting a broker costs seconds, not
 * milliseconds.
 *
 * <p>{{@code @ServiceConnection}} points the application at the container --
 * no bootstrap-servers property to override, and no chance of a test quietly
 * using the developer's own broker.
 *
 * <p>The latch is the part worth copying. Consumption is asynchronous, so an
 * assertion made straight after publishing races the consumer and fails about
 * one run in five. Waiting on a latch with a timeout either observes the
 * message or fails with a clear timeout.
 */
@SpringBootTest
@Import({name}MessagingIT.Containers.class)
class {name}MessagingIT {{

    private static final CountDownLatch RECEIVED = new CountDownLatch(1);
    private static final AtomicReference<{name}Event> LAST = new AtomicReference<>();

    @Autowired
    private {name}Publisher publisher;

    @Test
    void aPublishedEventIsConsumed() throws InterruptedException {{
        {name}Event event = new {name}Event("probe-1", Instant.parse("2024-01-01T00:00:00Z"));

        publisher.publish(event);

        assertThat(RECEIVED.await(30, TimeUnit.SECONDS))
                .as("the event should have been consumed within 30s")
                .isTrue();
        assertThat(LAST.get().id()).isEqualTo("probe-1");
    }}

    /**
     * A second listener on the same topic, in its own consumer group so it
     * does not compete with the application's listener for partitions --
     * two consumers in one group split the work and each message reaches
     * only one of them.
     */
    @KafkaListener(topics = "${{topics.{topic}:{topic}}}", groupId = "{topic}-it-probe")
    void record({name}Event event) {{
        LAST.set(event);
        RECEIVED.countDown();
    }}

    @TestConfiguration(proxyBeanMethods = false)
    static class Containers {{

        @Bean
        @ServiceConnection
        KafkaContainer kafka() {{
            return new KafkaContainer("apache/kafka:4.1.0")
                    .withStartupTimeout(Duration.ofMinutes(2));
        }}
    }}
}}
"#
    )
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
    format!(
        r#"package {pkg};

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.security.config.Customizer;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.web.SecurityFilterChain;

/**
 * Who may reach what, spelled out.
 *
 * <p>Written rather than inherited on purpose. Spring Boot's default chain
 * secures everything and prints a generated password at startup, which is a
 * good default and an opaque one -- and the usual reaction to it is a blanket
 * {{@code permitAll()}} that nobody revisits. A chain you can read is a chain
 * you can review.
 *
 * <p>Shaped for an API rather than a browser application. The three choices
 * below go together and are only safe together:
 *
 * <ul>
 *   <li>{{@code STATELESS}} -- no session is created, so there is no session
 *       cookie.
 *   <li>CSRF disabled -- CSRF is an attack on *ambient* credentials, meaning
 *       one the browser attaches automatically, like a session cookie. With
 *       no cookie there is nothing to ride on. Re-enable it the moment this
 *       application starts issuing one: form login, {{@code rememberMe}} and
 *       session-based auth all need it.
 *   <li>HTTP Basic -- honest placeholder. Replace it with the real scheme
 *       ({{@code oauth2ResourceServer}} for JWTs) rather than building a
 *       token check by hand.
 * </ul>
 */
@Configuration(proxyBeanMethods = false)
public class SecurityConfig {{

    @Bean
    public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {{
        return http.authorizeHttpRequests(
                        requests ->
                                requests
                                        // Liveness for a load balancer, which
                                        // cannot authenticate. Only `health` --
                                        // `env` and `heapdump` are not public.
                                        .requestMatchers("/actuator/health/**")
                                        .permitAll()
                                        // Default deny: a new endpoint is
                                        // protected until someone says
                                        // otherwise, which is the only default
                                        // that fails safe.
                                        .anyRequest()
                                        .authenticated())
                .sessionManagement(
                        session -> session.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
                .csrf(AbstractHttpConfigurer::disable)
                .httpBasic(Customizer.withDefaults())
                .build();
    }}
}}
"#
    )
}

fn security_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.nio.charset.StandardCharsets;
import java.util.Base64;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Both directions, because only one of them is usually checked.
 *
 * <p>A test that an authenticated request succeeds passes just as happily
 * against a chain with {{@code permitAll()}} on everything. The assertion that
 * an anonymous request is *rejected* is the one that notices when the rules
 * are loosened -- which is exactly the change nobody means to make
 * permanently.
 *
 * <p>The credentials are test-only properties and the request carries a real
 * {{@code Authorization}} header, rather than using
 * {{@code @WithMockUser}}. Two reasons: it exercises the actual
 * authentication filter instead of installing a {{@code SecurityContext}}
 * behind it, and {{@code @WithMockUser}} does not survive a
 * {{@code STATELESS}} chain anyway -- with no {{@code SecurityContext}}
 * repository, the context set by the test is never read back.
 */
@SpringBootTest(
        properties = {{
            "spring.security.user.name=probe",
            "spring.security.user.password=probe"
        }})
@AutoConfigureMockMvc
class SecurityConfigTest {{

    private static final String BASIC =
            "Basic "
                    + Base64.getEncoder()
                            .encodeToString("probe:probe".getBytes(StandardCharsets.UTF_8));

    @Autowired
    private MockMvcTester mvc;

    @Test
    void healthIsReachableWithoutCredentials() {{
        // A load balancer cannot authenticate. Needs `jails add actuator`
        // for the endpoint to exist at all.
        assertThat(mvc.get().uri("/actuator/health")).hasStatusOk();
    }}

    @Test
    void anythingElseRequiresCredentials() {{
        assertThat(mvc.get().uri("/anything")).hasStatus(401);
    }}

    @Test
    void anAuthenticatedRequestGetsThrough() {{
        // 404 rather than 401: the credentials were accepted and there is
        // simply nothing mapped at that path yet.
        assertThat(mvc.get().uri("/anything").header("Authorization", BASIC)).hasStatus(404);
    }}
}}
"#
    )
}
