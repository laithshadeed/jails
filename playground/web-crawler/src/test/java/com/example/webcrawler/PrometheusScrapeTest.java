package com.example.webcrawler;

import static org.assertj.core.api.Assertions.assertThat;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

/**
 * Proves the scrape endpoint actually serves, which the unit tests cannot.
 *
 * <p>Three separate things have to line up before Prometheus can read this
 * application -- the registry on the classpath, the endpoint in the exposure
 * list, and the meters registered -- and every one of them fails silently.
 * A missing registry is not an error, it is an endpoint that 404s; a narrowed
 * exposure list is not an error either. The symptom in all cases is a
 * dashboard that stays empty, noticed days later.
 *
 * <p>The probe sends test-only Basic credentials. Applications without the
 * security capability ignore the header; secured applications exercise the
 * real filter chain without making the scrape endpoint public.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest(
        webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT,
        properties = {
            "management.server.port=0",
            "app.security.dev.username=prometheus-probe",
            "app.security.dev.password=prometheus-probe"
        })
class PrometheusScrapeTest {

    private static final String BASIC =
            "Basic "
                    + Base64.getEncoder()
                            .encodeToString(
                                    "prometheus-probe:prometheus-probe"
                                            .getBytes(StandardCharsets.UTF_8));

    @Value("${local.management.port}") private int managementPort;

    private final HttpClient http = HttpClient.newHttpClient();

    private HttpResponse<String> get(String path) throws Exception {
        return http.send(
                HttpRequest.newBuilder(URI.create("http://127.0.0.1:" + managementPort + path))
                        .header("Authorization", BASIC)
                        .build(),
                HttpResponse.BodyHandlers.ofString());
    }

    @Test
    void theScrapeEndpointServesThisApplicationsMeters() throws Exception {
        var response = get("/management/prometheus");
        assertThat(response.statusCode()).isEqualTo(200);
        assertThat(response.body())
                // Micrometer renames dots to underscores for Prometheus, so
                // asserting the exported spelling also pins that translation.
                .contains("app_requests_handled")
                // And that the common tag reached a meter registered directly
                // on the registry -- the case a properties-only approach
                // misses, since `management.observations.key-values` tags
                // observations and a plain Counter is not one.
                .contains("application=");
    }

    @Test
    void theDangerousEndpointsStayUnexposed() throws Exception {
        // 4xx rather than 404: `jails add security` turns these into 401s.
        assertThat(get("/management/env").statusCode()).isBetween(400, 499);
        assertThat(get("/management/heapdump").statusCode()).isBetween(400, 499);
    }
}
