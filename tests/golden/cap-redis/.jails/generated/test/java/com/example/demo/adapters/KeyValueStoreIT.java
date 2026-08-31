package com.example.demo.adapters;

import java.time.Duration;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.testcontainers.containers.GenericContainer;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Against a real Redis, because the behaviour worth testing is Redis's.
 *
 * <p>An {@code IT}: Failsafe runs it in {@code verify}, not on every
 * {@code jails test}, since starting a container costs seconds.
 *
 * <p>{@code @ServiceConnection(name = "redis")} names the connection-details
 * factory explicitly rather than leaving Boot to infer it from the image.
 * Inference works only for image names it recognises, so a private mirror --
 * or, as here, a tag it does not expect -- fails at runtime with
 * {@code No ConnectionDetails found for source}, which reads like a missing
 * dependency rather than a naming problem. The explicit name is documented
 * and cannot drift.
 */
@SpringBootTest
@Import(KeyValueStoreIT.Containers.class)
class KeyValueStoreIT {

    @Autowired
    private KeyValueStore store;

    @Test
    void aValueSurvivesARoundTrip() {
        store.put("probe", "value");

        assertThat(store.get("probe")).contains("value");
    }

    @Test
    void anAbsentKeyIsEmptyRatherThanNull() {
        assertThat(store.get("never-written")).isEmpty();
    }

    @Test
    void removeReportsWhetherAnythingWasThere() {
        store.put("doomed", "value", Duration.ofMinutes(1));

        assertThat(store.remove("doomed")).isTrue();
        assertThat(store.remove("doomed")).isFalse();
        assertThat(store.get("doomed")).isEmpty();
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Containers {

        @Bean
        @ServiceConnection(name = "redis")
        GenericContainer<?> redis() {
            return new GenericContainer<>("redis:7-alpine").withExposedPorts(6379);
        }
    }
}
