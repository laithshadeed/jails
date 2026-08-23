package {{pkg}};

import java.time.Duration;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.context.annotation.Bean;
import org.testcontainers.kafka.KafkaContainer;

/**
 * One process-scoped Kafka broker for integration tests that actually publish
 * or consume records.
 *
 * <p>Import this configuration only on broker-backed tests. Ordinary Spring
 * contexts should not start Kafka or quietly connect to a developer broker on
 * {@code localhost:9092}.
 *
 * <p>{@code @ServiceConnection} supplies the dynamically mapped bootstrap
 * address to consumers, producers, and Kafka admin. The static container is
 * shared by every importing application context in this test JVM. Spring may
 * close those contexts independently, so {@link ProcessKafka#stop()} leaves
 * process-level cleanup to Testcontainers' Ryuk sidecar.
 */
@TestConfiguration(proxyBeanMethods = false)
public class {{KAFKA_TESTCONTAINERS_CONFIG}} {

    private static final KafkaContainer KAFKA = new ProcessKafka();

    @Bean
    @ServiceConnection
    @ConditionalOnProperty(
        name = "jails.testcontainers.kafka.enabled",
        matchIfMissing = true
    )
    KafkaContainer kafkaContainer() {
        return KAFKA;
    }

    private static final class ProcessKafka extends KafkaContainer {

        private ProcessKafka() {
            super("apache/kafka:4.1.0");
            withStartupTimeout(Duration.ofMinutes(2));
        }

        @Override
        public void stop() {
            // Deliberately process-scoped; Ryuk owns cleanup at JVM exit.
        }
    }
}
