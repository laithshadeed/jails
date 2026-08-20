package {{pkg}};

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.cache.annotation.Cacheable;
import org.springframework.context.annotation.Bean;
import org.springframework.stereotype.Component;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Proves caching is actually switched on.
 *
 * <p>Worth a test because the failure is silent: without {@code @EnableCaching}
 * a {@code @Cacheable} method simply runs every time, and nothing anywhere
 * reports a problem. Counting invocations is the only way to tell the two
 * states apart.
 */
@SpringBootTest
class CacheConfigTest {

    @Autowired
    private Counter counter;

    @Test
    void aSecondCallWithTheSameArgumentDoesNotRunTheMethod() {
        counter.reset();

        assertThat(counter.slow("a")).isEqualTo(1);
        assertThat(counter.slow("a")).isEqualTo(1);
        assertThat(counter.calls()).isEqualTo(1);

        // A different argument is a different cache key, so the method runs.
        assertThat(counter.slow("b")).isEqualTo(2);
        assertThat(counter.calls()).isEqualTo(2);
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        Counter counter() {
            return new Counter();
        }
    }

    /** Self-proxied through Spring, so {@code @Cacheable} actually applies. */
    @Component
    static class Counter {

        private final AtomicInteger calls = new AtomicInteger();

        @Cacheable("jails-cache-probe")
        public int slow(String key) {
            return calls.incrementAndGet();
        }

        int calls() {
            return calls.get();
        }

        void reset() {
            calls.set(0);
        }
    }
}
