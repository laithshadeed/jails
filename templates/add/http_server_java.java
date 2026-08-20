package {{pkg}};

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * A small HTTP server on the JDK's own {@code com.sun.net.httpserver} -- no
 * framework, no container, no dependency.
 *
 * <p>Handlers are pure functions from {@link Request} to {@link Response}, so
 * the interesting half can be unit-tested without any socket at all; this class
 * only maps them onto HTTP.
 *
 * <p>Requests are served on virtual threads, so a handler that blocks on I/O
 * costs a stack, not a platform thread.
 *
 * {@snippet :
 * try (var server = {{class}}.start(0, Map.of("/health", request -> Response.ok("{\"status\":\"up\"}")))) {
 *     var uri = URI.create("http://localhost:" + server.port() + "/health");
 * }
 * }
 */
public final class {{class}} implements AutoCloseable {

    /** Everything a handler is allowed to see. */
    public record Request(String method, String path, String query, String body) {}

    /** Everything a handler can say. JSON by default -- override for anything else. */
    public record Response(int status, String contentType, String body) {

        public static Response ok(String body) {
            return new Response(200, "application/json", body);
        }

        public static Response text(String body) {
            return new Response(200, "text/plain; charset=utf-8", body);
        }

        public static Response notFound() {
            return new Response(404, "application/json", "{\"error\":\"not found\"}");
        }

        public static Response badRequest(String message) {
            return new Response(400, "application/json", "{\"error\":\"" + escape(message) + "\"}");
        }

        /**
         * Escapes exactly what a JSON string body needs. Deliberately not a JSON
         * library: this class has no dependencies, and one interpolated message
         * does not justify adding one. Build real payloads with a real
         * serialiser -- {@code jails add json} gives you Jackson.
         */
        private static String escape(String text) {
            var out = new StringBuilder(text.length() + 16);
            for (var c : text.toCharArray()) {
                switch (c) {
                    case '"' -> out.append("\\\"");
                    case '\\' -> out.append("\\\\");
                    case '\n' -> out.append("\\n");
                    case '\r' -> out.append("\\r");
                    case '\t' -> out.append("\\t");
                    // Appended from a char rather than written as one literal:
                    // Java translates a backslash-u escape before it even lexes
                    // the file, and %04x is not four hex digits, so the obvious
                    // spelling is an "illegal unicode escape" at compile time.
                    // (Which applies to comments too -- hence this wording.)
                    default -> {
                        if (c < 0x20) {
                            out.append('\\').append("u%04x".formatted((int) c));
                        } else {
                            out.append(c);
                        }
                    }
                }
            }
            return out.toString();
        }
    }

    @FunctionalInterface
    public interface Handler {
        Response handle(Request request);
    }

    private final HttpServer http;
    private final ExecutorService requests;

    private {{class}}(HttpServer http, ExecutorService requests) {
        this.http = http;
        this.requests = requests;
    }

    /**
     * Binds and starts. Pass port 0 to let the OS pick a free one and read it
     * back from {@link #port()} -- which is what makes tests safe to run in
     * parallel, and CI safe from whatever else is listening on 8080.
     */
    public static {{class}} start(int port, Map<String, Handler> routes) {
        try {
            var http = HttpServer.create(new InetSocketAddress(port), 0);
            routes.forEach((path, handler) -> http.createContext(path, exchange -> dispatch(exchange, handler)));
            var requests = Executors.newVirtualThreadPerTaskExecutor();
            http.setExecutor(requests);
            http.start();
            return new {{class}}(http, requests);
        } catch (IOException error) {
            throw new UncheckedIOException("could not start the server on port " + port, error);
        }
    }

    public int port() {
        return http.getAddress().getPort();
    }

    private static void dispatch(HttpExchange exchange, Handler handler) throws IOException {
        try (exchange) {
            var uri = exchange.getRequestURI();
            Response response;
            try (var in = exchange.getRequestBody()) {
                var body = new String(in.readAllBytes(), StandardCharsets.UTF_8);
                var request = new Request(exchange.getRequestMethod(), uri.getPath(), uri.getQuery(), body);
                // A handler that throws must not leave the connection hanging:
                // the client would block until it timed out, with nothing said.
                response = handle(handler, request);
            }

            var bytes = response.body().getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().set("Content-Type", response.contentType());
            exchange.sendResponseHeaders(response.status(), bytes.length);
            try (var out = exchange.getResponseBody()) {
                out.write(bytes);
            }
        }
    }

    private static Response handle(Handler handler, Request request) {
        try {
            return handler.handle(request);
        } catch (RuntimeException error) {
            // The client gets nothing useful (deliberately -- an exception
            // message can carry internals), but swallowing it outright leaves
            // nobody anything to debug from. Swap in a logger when you add one.
            System.err.println("handler failed for " + request.method() + " " + request.path());
            error.printStackTrace();
            return new Response(500, "application/json", "{\"error\":\"internal error\"}");
        }
    }

    /**
     * Stops accepting connections and shuts the request executor down.
     *
     * <p>Both halves matter: {@link HttpServer#stop} does <em>not</em> shut down
     * an executor the caller supplied, so stopping without this leaks one per
     * server -- which a test that starts a server per case does many times over.
     */
    @Override
    public void close() {
        http.stop(0);
        requests.close();
    }
}
