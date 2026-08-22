package {{pkg}};

import {{messaging}}.{{event}}Event;
import com.sun.net.httpserver.HttpServer;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
{{imports}}{{disabled_import}}import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

{{annotation}}class {{name}}HttpOutboxSinkTest {

    private HttpServer server;

    @AfterEach
    void stopServer() {
        if (server != null) server.stop(0);
    }

    @Test
    void repeatedAttemptsCarryTheSameIdempotencyKeyAndTypedJson() throws IOException {
        List<String> keys = new CopyOnWriteArrayList<>();
        List<String> bodies = new CopyOnWriteArrayList<>();
        server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/deliver", exchange -> {
            keys.add(exchange.getRequestHeaders().getFirst("Idempotency-Key"));
            bodies.add(new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8));
            exchange.sendResponseHeaders(202, -1);
            exchange.close();
        });
        server.createContext("/reject", exchange -> {
            exchange.sendResponseHeaders(503, -1);
            exchange.close();
        });
        server.start();

        var event = new {{event}}Event(
                {{args}});
        var accepted = sink("/deliver");
        accepted.deliver(event);
        accepted.deliver(event);

        assertThat(keys).containsExactly(String.valueOf(event.id()), String.valueOf(event.id()));
        assertThat(bodies).hasSize(2).allSatisfy(body ->
                assertThat(body).contains(String.valueOf(event.id())));
        assertThatThrownBy(() -> sink("/reject").deliver(event))
                .isInstanceOf({{name}}HttpOutboxSink.DeliveryException.class)
                .hasMessageContaining("503");
    }

    private {{name}}HttpOutboxSink sink(String path) {
        return new {{name}}HttpOutboxSink(
                "http://" + server.getAddress().getHostString() + ":" + server.getAddress().getPort() + path,
                "test-token", 500, 1000, new SimpleMeterRegistry());
    }
}
