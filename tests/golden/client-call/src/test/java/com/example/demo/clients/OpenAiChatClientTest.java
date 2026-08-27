package com.example.demo.clients;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.ChatReply;
import com.example.demo.domain.ChatRequest;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.net.InetSocketAddress;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.DynamicPropertyRegistry;
import org.springframework.test.context.DynamicPropertySource;

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
@Disabled("todo: build the ChatRequest this call needs, then delete this @Disabled")
@SpringBootTest
class OpenAiChatClientTest {

    private static HttpServer server;

    @Autowired
    private OpenAiChatClient client;

    @BeforeAll
    static void startStub() throws IOException {
        // Port 0: the OS picks a free one, so parallel runs cannot collide.
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext("/v1/chat/completions", exchange -> {
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
                "spring.http.serviceclient.open-ai-chat.base-url",
                () -> "http://localhost:" + server.getAddress().getPort());
    }

    @Test
    void theCallReachesTheService() {
        assertThat(client.call(sample())).isNotNull();
    }

    private static ChatRequest sample() {
        throw new UnsupportedOperationException(
                "todo: build the ChatRequest this call sends");
    }
}
