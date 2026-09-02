package com.example.demo.api;

import static org.assertj.core.api.Assertions.assertThat;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Map;
import org.junit.jupiter.api.Test;

/** End-to-end over a real socket, on an ephemeral port so nothing collides. */
class ServerTest {

    private static final Map<String, Server.Handler> ROUTES = Map.of(
            "/health", request -> Server.Response.ok("{\"status\":\"up\"}"),
            "/echo", request -> Server.Response.text(request.method() + " " + request.body()),
            "/boom", request -> {
                throw new IllegalStateException("handler blew up");
            });

    private HttpResponse<String> call(int port, String path, String body) throws Exception {
        var request = HttpRequest.newBuilder(URI.create("http://localhost:" + port + path))
                .method(body == null ? "GET" : "POST", body == null
                        ? HttpRequest.BodyPublishers.noBody()
                        : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }
    }

    @Test
    void servesARegisteredRoute() throws Exception {
        try (var server = Server.start(0, ROUTES)) {
            var response = call(server.port(), "/health", null);

            assertThat(response.statusCode()).isEqualTo(200);
            assertThat(response.body()).contains("up");
            assertThat(response.headers().firstValue("Content-Type")).hasValue("application/json");
        }
    }

    @Test
    void handsTheHandlerTheMethodAndBody() throws Exception {
        try (var server = Server.start(0, ROUTES)) {
            assertThat(call(server.port(), "/echo", "hello").body()).isEqualTo("POST hello");
        }
    }

    @Test
    void answersUnknownPathsWithFourOhFour() throws Exception {
        try (var server = Server.start(0, ROUTES)) {
            assertThat(call(server.port(), "/nope", null).statusCode()).isEqualTo(404);
        }
    }

    /** A throwing handler must still answer, or the client just hangs. */
    @Test
    void turnsAHandlerExceptionIntoAFiveHundred() throws Exception {
        try (var server = Server.start(0, ROUTES)) {
            assertThat(call(server.port(), "/boom", null).statusCode()).isEqualTo(500);
        }
    }

    @Test
    void picksAFreePortWhenAskedForZero() {
        try (var server = Server.start(0, ROUTES)) {
            assertThat(server.port()).isPositive();
        }
    }
}
