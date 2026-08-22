package {{pkg}};

import com.sun.net.httpserver.HttpServer;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.Set;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class Safe{{name}}FetcherTest {

    private static HttpServer server;

    @BeforeAll
    static void startServer() throws IOException {
        server = HttpServer.create(new InetSocketAddress(InetAddress.getLoopbackAddress(), 0), 0);
        server.createContext("/ok", exchange -> respond(exchange, 200, "text/html; charset=utf-8", "<p>ok</p>"));
        server.createContext("/large", exchange -> respond(exchange, 200, "text/html", "x".repeat(129)));
        server.createContext("/missing", exchange -> respond(exchange, 404, "text/html", "missing"));
        server.createContext("/other-host", exchange -> {
            exchange.getResponseHeaders().add(
                    "Location", "http://localhost:" + server.getAddress().getPort() + "/ok");
            exchange.sendResponseHeaders(302, -1);
            exchange.close();
        });
        server.start();
    }

    @AfterAll
    static void stopServer() {
        server.stop(0);
    }

    @Test
    void fetchesThroughAPinnedAddressAndDefensivelyCopiesTheBody() throws Exception {
        var fetcher = localFetcher(1024);
        var result = fetcher.fetch(uri("/ok"));

        assertThat(result.statusCode()).isEqualTo(200);
        assertThat(result.contentType()).isEqualTo("text/html");
        assertThat(new String(result.body(), StandardCharsets.UTF_8)).isEqualTo("<p>ok</p>");
        byte[] first = result.body();
        first[0] = 0;
        assertThat(result.body()[0]).isNotZero();
    }

    @Test
    void rejectsPrivateAddressesInProductionPolicy() throws Exception {
        var fetcher = new Safe{{name}}Fetcher(
                Duration.ofSeconds(1),
                Duration.ofSeconds(1),
                1024,
                1,
                "test",
                "text/html",
                ignored -> new InetAddress[] {InetAddress.getLoopbackAddress()},
                false,
                new SimpleMeterRegistry());

        assertThatThrownBy(() -> fetcher.fetch(URI.create("http://public.example/")))
                .isInstanceOf({{name}}Fetcher.FetchException.class)
                .hasMessageContaining("private or reserved");
    }

    @Test
    void enforcesTheByteLimitAndRejectsCrossHostRedirects() {
        assertThatThrownBy(() -> localFetcher(128).fetch(uri("/large")))
                .isInstanceOf({{name}}Fetcher.FetchException.class)
                .hasMessageContaining("exceeds 128 bytes");
        assertThatThrownBy(() -> localFetcher(1024).fetch(uri("/other-host")))
                .isInstanceOf({{name}}Fetcher.FetchException.class)
                .hasMessageContaining("original host");
    }

    @Test
    void selectedProtocolStatusesCanBeObservedWithoutWeakeningTheDefault() {
        var fetcher = localFetcher(1024);

        assertThatThrownBy(() -> fetcher.fetch(uri("/missing")))
                .isInstanceOf({{name}}Fetcher.FetchException.class)
                .hasMessageContaining("HTTP 404");
        assertThat(fetcher.fetch(uri("/missing"), Set.of(404)).statusCode()).isEqualTo(404);
    }

    private static Safe{{name}}Fetcher localFetcher(int maxBytes) {
        return new Safe{{name}}Fetcher(
                Duration.ofSeconds(1),
                Duration.ofSeconds(1),
                maxBytes,
                2,
                "test",
                "text/html",
                ignored -> new InetAddress[] {InetAddress.getLoopbackAddress()},
                true,
                new SimpleMeterRegistry());
    }

    private static URI uri(String path) {
        return URI.create("http://127.0.0.1:" + server.getAddress().getPort() + path);
    }

    private static void respond(
            com.sun.net.httpserver.HttpExchange exchange,
            int status,
            String contentType,
            String value)
            throws IOException {
        byte[] body = value.getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().add("Content-Type", contentType);
        exchange.sendResponseHeaders(status, body.length);
        try (var output = exchange.getResponseBody()) {
            output.write(body);
        }
    }
}
