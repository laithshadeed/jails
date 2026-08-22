package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

/**
 * Pins the endpoints that are exposed, both ways.
 *
 * <p>The second assertion is the one that earns its place: `management.
 * endpoints.web.exposure.include` is a list people widen to `*` under time
 * pressure, and `*` publishes heap dumps and the resolved environment --
 * credentials included -- to anything that can reach the port. A test that
 * fails when that happens is cheaper than noticing in production.
 */
@SpringBootTest(properties = "management.server.port=")
@AutoConfigureMockMvc
class ActuatorEndpointsTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void healthIsExposed() {
        assertThat(mvc.get().uri("/management/health")).hasStatusOk();
    }

    @Test
    void everythingElseStaysUnexposed() {
        // 4xx rather than 404 specifically: an unexposed endpoint is a 404,
        // but once `jails add security` is in the project it becomes a 401
        // instead. Both mean "not available"; pinning 404 would make this
        // test fail the day the application is secured, which is exactly
        // backwards.
        assertThat(mvc.get().uri("/management/env")).hasStatus4xxClientError();
        assertThat(mvc.get().uri("/management/heapdump")).hasStatus4xxClientError();
    }
}
