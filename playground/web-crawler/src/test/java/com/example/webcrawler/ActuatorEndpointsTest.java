package com.example.webcrawler;

import static org.assertj.core.api.Assertions.assertThat;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.web.server.LocalServerPort;
import org.springframework.context.annotation.Import;

/**
 * Pins the endpoints that are exposed, both ways.
 *
 * <p>The second assertion is the one that earns its place: `management.
 * endpoints.web.exposure.include` is a list people widen to `*` under time
 * pressure, and `*` publishes heap dumps and the resolved environment --
 * credentials included -- to anything that can reach the port. A test that
 * fails when that happens is cheaper than noticing in production.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest(
        webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT,
        properties = {
            "management.server.port=0",
            "app.security.dev.username=prometheus-probe",
            "app.security.dev.password=prometheus-probe"
        })
class ActuatorEndpointsTest {

    @LocalServerPort private int applicationPort;
    @Value("${local.management.port}") private int managementPort;

    private final HttpClient http = HttpClient.newHttpClient();

    private int status(String path) throws Exception {
        return http.send(
                        HttpRequest.newBuilder(
                                        URI.create("http://127.0.0.1:" + managementPort + path))
                                .build(),
                        HttpResponse.BodyHandlers.discarding())
                .statusCode();
    }

    @Test
    void healthIsExposedOnASeparateManagementConnector() throws Exception {
        assertThat(managementPort).isNotEqualTo(applicationPort);
        assertThat(status("/management/health")).isEqualTo(200);
    }

    @Test
    void everythingElseStaysUnexposed() throws Exception {
        // 4xx rather than 404 specifically: an unexposed endpoint is a 404,
        // but once `jails add security` is in the project it becomes a 401
        // instead. Both mean "not available"; pinning 404 would make this
        // test fail the day the application is secured, which is exactly
        // backwards.
        assertThat(status("/management/env")).isBetween(400, 499);
        assertThat(status("/management/heapdump")).isBetween(400, 499);
    }
}
