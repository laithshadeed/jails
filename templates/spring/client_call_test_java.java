package {{pkg}};

import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.net.InetSocketAddress;
{{imports}}{{disabled_import}}import org.junit.jupiter.api.AfterAll;
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
 * <p>The stub is the JDK's own {@link HttpServer} -- no extra dependency, and
 * a real socket, so this exercises serialization, status handling and the
 * configured base URL rather than a mock's idea of them.
 *
 * <p>{@code @DynamicPropertySource} resolves the port after the server binds
 * and before the context starts, which no static property file can do.
 */
{{disabled}}@SpringBootTest
class {{name}}ClientTest {

    private static HttpServer server;

    @Autowired
    private {{name}}Client client;

    @BeforeAll
    static void startStub() throws IOException {
        // Port 0: the OS picks a free one, so parallel runs cannot collide.
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext("{{path}}", exchange -> {
            byte[] body = "{}".getBytes(java.nio.charset.StandardCharsets.UTF_8);
            exchange.getResponseHeaders().add("Content-Type", "application/json");
            exchange.sendResponseHeaders(200, body.length);
            try (var out = exchange.getResponseBody()) {
                out.write(body);
            }
        });
        server.start();
    }

    @AfterAll
    static void stopStub() {
        server.stop(0);
    }

    @DynamicPropertySource
    static void baseUrl(DynamicPropertyRegistry registry) {
        registry.add(
                "spring.http.serviceclient.{{group}}.base-url",
                () -> "http://localhost:" + server.getAddress().getPort());
    }

    @Test
    void theCallReachesTheService() {
        assertThat(client.call({{argument}})).isNotNull();
    }
{{sample}}}
