package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

/**
 * Proves the scrape endpoint actually serves, which the unit tests cannot.
 *
 * <p>Three separate things have to line up before Prometheus can read this
 * application -- the registry on the classpath, the endpoint in the exposure
 * list, and the meters registered -- and every one of them fails silently.
 * A missing registry is not an error, it is an endpoint that 404s; a narrowed
 * exposure list is not an error either. The symptom in all cases is a
 * dashboard that stays empty, noticed days later.
 */
@SpringBootTest
@AutoConfigureMockMvc
class PrometheusScrapeTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void theScrapeEndpointServesThisApplicationsMeters() {
        assertThat(mvc.get().uri("/actuator/prometheus"))
                .hasStatusOk()
                .bodyText()
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
    void theDangerousEndpointsStayUnexposed() {
        // 4xx rather than 404: `jails add security` turns these into 401s.
        assertThat(mvc.get().uri("/actuator/env")).hasStatus4xxClientError();
        assertThat(mvc.get().uri("/actuator/heapdump")).hasStatus4xxClientError();
    }
}
