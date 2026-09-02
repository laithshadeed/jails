package {{pkg}};

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
class {{name}}HandlerTest {

    private HttpServer server;

    /** A stand-in service: enough behaviour to exercise every status code. */
    private final {{name}}Handler.Service service = new {{name}}Handler.Service() {
        @Override
        public String list(int offset, int limit) {
            return "[{\"id\":\"a\"}]";
        }

        @Override
        public Optional<String> find(String id) {
            return id.equals("a") ? Optional.of("{\"id\":\"a\"}") : Optional.empty();
        }

        @Override
        public String create(String body) {
            if (body.contains("\"invalid\"")) {
                throw new IllegalArgumentException("id must not be blank");
            }
            return body;
        }
    };

    @BeforeEach
    void start() throws Exception {
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext({{name}}Handler.PATH, new {{name}}Handler(service));
        server.setExecutor(Executors.newVirtualThreadPerTaskExecutor());
        server.start();
    }

    @AfterEach
    void stop() {
        server.stop(0);
    }

    private HttpResponse<String> send(String path, String body) throws Exception {
        var uri = URI.create("http://localhost:" + server.getAddress().getPort() + path);
        var request = HttpRequest.newBuilder(uri)
                .method(
                        body == null ? "GET" : "POST",
                        body == null ? HttpRequest.BodyPublishers.noBody() : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }
    }

    @Test
    void listsTheCollection() throws Exception {
        var response = send("{{path}}", null);

        assertThat(response.statusCode()).isEqualTo(200);
        assertThat(response.body()).contains("\"id\":\"a\"");
    }

    @Test
    void findsOneById() throws Exception {
        assertThat(send("{{path}}/a", null).statusCode()).isEqualTo(200);
    }

    @Test
    void answersFourOhFourForAnUnknownId() throws Exception {
        var response = send("{{path}}/nope", null);

        assertThat(response.statusCode()).isEqualTo(404);
        assertThat(response.body()).contains("not_found");
    }

    @Test
    void answersFourHundredForABodyThatIsNotJson() throws Exception {
        var response = send("{{path}}", "not json");

        assertThat(response.statusCode()).isEqualTo(400);
        assertThat(response.body()).contains("bad_request");
    }

    /** Well-formed but rejected by the domain is 422, not 400. */
    @Test
    void answersFourTwentyTwoWhenTheDomainRejectsIt() throws Exception {
        var response = send("{{path}}", "{\"invalid\":true}");

        assertThat(response.statusCode()).isEqualTo(422);
        assertThat(response.body()).contains("unprocessable");
    }

    @Test
    void answersFourHundredForANonNumericPageWindow() throws Exception {
        assertThat(send("{{path}}?offset=x", null).statusCode()).isEqualTo(400);
    }
}
