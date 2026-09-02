package com.example.demo;

import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * A {@link SimpleMeterRegistry} rather than a Spring context: the thing
 * being tested is that the meters exist under the names other systems will
 * query, and that needs no application.
 *
 * <p>Worth pinning because a renamed metric breaks a dashboard silently --
 * nothing fails, the graph just goes flat.
 */
class AppMetricsTest {

    private final SimpleMeterRegistry registry = new SimpleMeterRegistry();
    private final AppMetrics metrics = new AppMetrics(registry);

    @Test
    void theCounterIsRegisteredBeforeAnythingIncrementsIt() {
        // Registered eagerly, so a scrape taken before the first request
        // reports zero rather than nothing at all.
        assertThat(registry.find("app.requests.handled").counter()).isNotNull();
        assertThat(registry.find("app.requests.handled").counter().count()).isZero();

        metrics.requestHandled();

        assertThat(registry.find("app.requests.handled").counter().count()).isEqualTo(1.0);
    }

    @Test
    void theTimerRecordsAndReturnsTheResult() {
        String result = metrics.timed(() -> "done");

        assertThat(result).isEqualTo("done");
        assertThat(registry.find("app.work.duration").timer().count()).isEqualTo(1L);
    }
}
