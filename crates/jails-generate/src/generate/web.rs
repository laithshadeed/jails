//! The HTTP surface: the bare `controller`/`service` stubs and the
//! `handler` that gives one resource a real, thin endpoint.
//!
//! Both stubs are package-private. Spring instantiates and calls them by
//! reflection, so `public` buys nothing and only widens what other packages
//! can compile against.

// ---- standalone stub templates (ported from springgen.nvim) ----

pub(super) fn interface_java(pkg: &str, name: &str) -> String {
    format!("package {pkg};\n\npublic interface {name} {{\n}}\n")
}

pub(super) fn integration_test_java(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

@Disabled("todo: wire the real integration boundary")
class {name}IT {{

    @Test
    void worksEndToEnd() {{
        throw new UnsupportedOperationException("todo");
    }}
}}
"#
    )
}

pub(super) fn stub_controller(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("generate/stub_controller.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("route", &name.to_lowercase()),
        ],
    )
}

pub(super) fn stub_service(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.stereotype.Component;

/**
 * Package-private: Spring injects this by type, and nothing outside this
 * package should be compiling against it. Widen it when something genuinely
 * outside needs it, not before.
 */
@Component
class {name}Service {{
}}
"#
    )
}

pub(super) fn stub_class(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

public final class {name} {{
}}
"#
    )
}

/// The companion test for `generate class`.
///
/// It used to construct the class and assert `isNotNull()`, which is three
/// bad things at once: it **passes while the class is entirely broken** (one
/// real project reported 39 green tests over a repository that could not read
/// or write), it inflates the count so the suite looks covered, and passing
/// `null` for a constructor argument teaches that as the pattern. `java.md`
/// §7 -- "don't test getters, records' `equals`, or Spring's wiring" -- is the
/// same rule stated generally.
///
/// So it is `@Disabled` with a name that says what to prove. That is jails'
/// existing idiom for "you have to finish this" (the field-spec sample
/// problem emits `@Disabled` tests for the same reason), and it fixes every
/// one of the three defects: a disabled test is reported as skipped rather
/// than counted as green, so it is visible in the surefire output and cannot
/// masquerade as coverage.
///
/// Deliberately **not** a failing test, which was the other candidate: `jails
/// new` followed by `jails check` would then be red on a project where
/// nothing is wrong, and a red build that is expected is a red build nobody
/// reads.
///
/// The construction is kept. A bare class has an implicit no-arg constructor,
/// so this compiles the moment it is written, and stops compiling the day a
/// real constructor arrives -- which is the prompt to write the real
/// assertion.
pub(super) fn class_test(pkg: &str, name: &str) -> String {
    let victim = lower_first(name);
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class {name}Test {{

    @Test
    @Disabled("todo: state what {name} is supposed to do, then assert it")
    void todo() {{
        {name} {victim} = new {name}();

        // Replace this with the behaviour {name} exists for. Asserting that
        // it is not null would pass while the class is entirely broken.
    }}
}}
"#
    )
}

pub fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn stub_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

class {name}Test {{

    @Test
    void shouldDoSomething() {{
        assertThat(true).isTrue();
    }}

}}
"#
    )
}

// ---- companion tests for the bare `generate controller`/`service` stubs. ----

/// The controller's companion test, written against `MockMvcTester` rather
/// than plain `MockMvc`.
///
/// `MockMvcTester` is Spring's AssertJ entry point (`@AutoConfigureMockMvc`
/// contributes one whenever AssertJ is on the classpath, which
/// `spring-boot-starter-test` guarantees). Three things it buys over
/// `mockMvc.perform(get(...)).andExpect(status().isOk())`: the request and
/// the assertions are one fluent chain instead of two families of static
/// imports, an unresolved exception is reported as a failed assertion
/// instead of being thrown, and the test method needs no `throws Exception`
/// -- which is what makes the generated body a thing you extend rather than
/// a thing you first have to reshape.
pub(super) fn controller_stub_test(pkg: &str, name: &str, mockmvc_import: &str) -> String {
    crate::template::render(
        crate::template_here!("generate/controller_stub_test.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("mockmvc_import", mockmvc_import),
            ("route", &name.to_lowercase()),
        ],
    )
}

pub(super) fn service_stub_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

class {name}ServiceTest {{

    @Test
    void instantiates() {{
        assertThat(new {name}Service()).isNotNull();
    }}
}}
"#
    )
}

// ---- handler: HTTP for one resource, thin by construction. ----

/// `WorkItem` -> `/work-items`. The URL convention is kebab-case and plural,
/// and deriving it beats making every caller remember to type it.
///
/// Through `sql::table_name`, not a second pluraliser: this function used to
/// append a bare `s`, so `g handler Category` served `/categorys` while the
/// very same resource's table was `categories` -- and the Spring scaffold's
/// controller, which does go through `table_name`, disagreed with the
/// framework-free handler about the URL of the same thing.
pub fn resource_path(name: &str) -> String {
    format!("/{}", crate::sql::table_name(name).replace('_', "-"))
}

pub(super) fn handler_java(pkg: &str, name: &str, extra: &str) -> String {
    let path = resource_path(name);
    format!(
        r#"package {pkg};

{extra}import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;

/**
 * HTTP for the {{@code {path}}} resource.
 *
 * <p>Thin by construction: this class binds, routes, and maps outcomes to
 * status codes. It holds no rules of its own, so the same {{@link Service}} can
 * be driven from the CLI without any of this.
 *
 * <p>{{@link Service}} deals in JSON strings because a scaffold cannot know
 * your types. Narrowing it to real ones is the first thing worth doing here.
 *
 * <p>Status codes are the contract:
 * <ul>
 *   <li>400 -- the body is not JSON, or a query parameter is not a number
 *   <li>404 -- no such resource
 *   <li>422 -- well-formed, but the domain rejected it
 * </ul>
 */
public final class {name}Handler implements HttpHandler {{

    /** The path this handler is registered under. */
    public static final String PATH = "{path}";

    /** What this handler needs from the application behind it. */
    public interface Service {{

        /** @return a JSON array of items, never null. */
        String list(int offset, int limit);

        /** @return the item as JSON, or empty when there is no such id. */
        Optional<String> find(String id);

        /**
         * @param body the raw request body
         * @return the created item as JSON
         * @throws IllegalArgumentException when the domain rejects it -- becomes a 422
         */
        String create(String body);
    }}

    private final Service service;

    public {name}Handler(Service service) {{
        this.service = Objects.requireNonNull(service, "service is required");
    }}

    @Override
    public void handle(HttpExchange exchange) throws IOException {{
        try (exchange) {{
            var path = exchange.getRequestURI().getPath();
            var id = idFrom(path);

            var response =
                    switch (exchange.getRequestMethod()) {{
                        case "GET" -> id.isEmpty() ? list(exchange) : find(id);
                        case "POST" -> create(body(exchange));
                        default -> error(405, "method_not_allowed", "use GET or POST");
                    }};

            send(exchange, response);
        }}
    }}

    /** The trailing path segment, or empty for a request against the collection. */
    private String idFrom(String path) {{
        var rest = path.length() > PATH.length() ? path.substring(PATH.length()) : "";
        return rest.startsWith("/") ? rest.substring(1) : rest;
    }}

    private Response list(HttpExchange exchange) {{
        var query = query(exchange);
        try {{
            var offset = Integer.parseInt(query.getOrDefault("offset", "0"));
            var limit = Integer.parseInt(query.getOrDefault("limit", "50"));
            return new Response(200, service.list(offset, limit));
        }} catch (NumberFormatException malformed) {{
            return error(400, "bad_request", "offset and limit must be whole numbers");
        }}
    }}

    private Response find(String id) {{
        return service.find(id)
                .map(json -> new Response(200, json))
                .orElseGet(() -> error(404, "not_found", "no {path} with id " + id));
    }}

    private Response create(String body) {{
        if (body.isBlank() || !body.stripLeading().startsWith("{{")) {{
            return error(400, "bad_request", "expected a JSON object");
        }}
        try {{
            return new Response(201, service.create(body));
        }} catch (IllegalArgumentException rejected) {{
            // Well-formed but wrong: the client sent something the domain will
            // not accept, which is 422 rather than 400.
            return error(422, "unprocessable", rejected.getMessage());
        }}
    }}

    /** An {{@link ApiError}} rendered as the response body. */
    private Response error(int status, String code, String message) {{
        var envelope = new ApiError(code, message == null ? code : message, Map.of());
        return new Response(
                status,
                "{{\"code\":\"" + envelope.code() + "\",\"message\":\"" + envelope.message() + "\"}}");
    }}

    private record Response(int status, String body) {{}}

    private static String body(HttpExchange exchange) throws IOException {{
        try (var in = exchange.getRequestBody()) {{
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }}
    }}

    private static Map<String, String> query(HttpExchange exchange) {{
        var raw = exchange.getRequestURI().getQuery();
        if (raw == null || raw.isBlank()) {{
            return Map.of();
        }}
        var parsed = new java.util.LinkedHashMap<String, String>();
        for (var pair : raw.split("&")) {{
            var split = pair.split("=", 2);
            if (split.length == 2) {{
                parsed.put(split[0], split[1]);
            }}
        }}
        return Map.copyOf(parsed);
    }}

    private static void send(HttpExchange exchange, Response response) throws IOException {{
        var bytes = response.body().getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.sendResponseHeaders(response.status(), bytes.length);
        try (var out = exchange.getResponseBody()) {{
            out.write(bytes);
        }}
    }}
}}
"#
    )
}

pub(super) fn handler_test(pkg: &str, name: &str) -> String {
    let path = resource_path(name);
    format!(
        r#"package {pkg};

import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.net.InetSocketAddress;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Optional;
import java.util.concurrent.Executors;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Drives the handler over a real loopback socket, because the interesting
 * half -- status codes, bodies, headers -- only exists once HTTP is involved.
 *
 * <p>Port 0 lets the OS pick a free one, so these tests are safe to run in
 * parallel and safe from whatever else is on 8080.
 */
class {name}HandlerTest {{

    private HttpServer server;

    /** A stand-in service: enough behaviour to exercise every status code. */
    private final {name}Handler.Service service = new {name}Handler.Service() {{
        @Override
        public String list(int offset, int limit) {{
            return "[{{\"id\":\"a\"}}]";
        }}

        @Override
        public Optional<String> find(String id) {{
            return id.equals("a") ? Optional.of("{{\"id\":\"a\"}}") : Optional.empty();
        }}

        @Override
        public String create(String body) {{
            if (body.contains("\"invalid\"")) {{
                throw new IllegalArgumentException("id must not be blank");
            }}
            return body;
        }}
    }};

    @BeforeEach
    void start() throws Exception {{
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext({name}Handler.PATH, new {name}Handler(service));
        server.setExecutor(Executors.newVirtualThreadPerTaskExecutor());
        server.start();
    }}

    @AfterEach
    void stop() {{
        server.stop(0);
    }}

    private HttpResponse<String> send(String path, String body) throws Exception {{
        var uri = URI.create("http://localhost:" + server.getAddress().getPort() + path);
        var request = HttpRequest.newBuilder(uri)
                .method(
                        body == null ? "GET" : "POST",
                        body == null ? HttpRequest.BodyPublishers.noBody() : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {{
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }}
    }}

    @Test
    void listsTheCollection() throws Exception {{
        var response = send("{path}", null);

        assertThat(response.statusCode()).isEqualTo(200);
        assertThat(response.body()).contains("\"id\":\"a\"");
    }}

    @Test
    void findsOneById() throws Exception {{
        assertThat(send("{path}/a", null).statusCode()).isEqualTo(200);
    }}

    @Test
    void answersFourOhFourForAnUnknownId() throws Exception {{
        var response = send("{path}/nope", null);

        assertThat(response.statusCode()).isEqualTo(404);
        assertThat(response.body()).contains("not_found");
    }}

    @Test
    void answersFourHundredForABodyThatIsNotJson() throws Exception {{
        var response = send("{path}", "not json");

        assertThat(response.statusCode()).isEqualTo(400);
        assertThat(response.body()).contains("bad_request");
    }}

    /** Well-formed but rejected by the domain is 422, not 400. */
    @Test
    void answersFourTwentyTwoWhenTheDomainRejectsIt() throws Exception {{
        var response = send("{path}", "{{\"invalid\":true}}");

        assertThat(response.statusCode()).isEqualTo(422);
        assertThat(response.body()).contains("unprocessable");
    }}

    @Test
    void answersFourHundredForANonNumericPageWindow() throws Exception {{
        assertThat(send("{path}?offset=x", null).statusCode()).isEqualTo(400);
    }}
}}
"#
    )
}
