package {{pkg}};

import java.time.Duration;
{{event_imports}}{{disabled_import}}
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.kafka.annotation.KafkaListener;
import org.testcontainers.kafka.KafkaContainer;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Publishes through a real broker and waits for it to come back.
 *
 * <p>An {@code IT}, so Failsafe runs it in {@code verify} rather than on
 * every {@code jails test}: starting a broker costs tens of seconds.
 *
 * <p>{@code @ServiceConnection} points the application at the container --
 * no bootstrap-servers property to override, and no chance of a test quietly
 * using the developer's own broker.
 *
 * <p>The latch is the part worth copying. Consumption is asynchronous, so an
 * assertion made straight after publishing races the consumer and fails about
 * one run in five. Waiting on a latch with a timeout either observes the
 * message or fails saying so.
 */
{{disabled}}@SpringBootTest(properties = "spring.kafka.consumer.properties.group.protocol=classic")
@Import({{name}}MessagingIT.Containers.class)
class {{name}}MessagingIT {

    @Autowired
    private {{name}}Publisher publisher;

    @Autowired
    private Probe probe;

    @Test
    void aPublishedEventIsConsumed() throws InterruptedException {
        {{name}}Event event = new {{name}}Event({{event_args}});

        publisher.publish(event);

        assertThat(probe.received.await(30, TimeUnit.SECONDS))
                .as("the event should have been consumed within 30s")
                .isTrue();
        assertThat(probe.last.get().id()).isEqualTo({{expected_id}});
    }

    /**
     * The probe has to be a *bean*, not a method on the test class.
     *
     * <p>{@code @KafkaListener} is registered by a bean post-processor, and
     * a test instance is not a bean -- Spring creates it and injects into it,
     * but never processes its annotations. A listener declared on the test
     * class is therefore silently never subscribed, and the only symptom is a
     * latch that times out with nothing in the log to explain it.
     */
    static class Probe {

        private final CountDownLatch received = new CountDownLatch(1);
        private final AtomicReference<{{name}}Event> last = new AtomicReference<>();

        /**
         * Its own consumer group, so it does not compete with the
         * application's listener: two consumers in one group split the
         * partitions and each message reaches only one of them.
         */
        @KafkaListener(topics = "${topics.{{topic}}:{{topic}}}", groupId = "{{topic}}-it-probe")
        void on({{name}}Event event) {
            last.set(event);
            received.countDown();
        }
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Containers {

        @Bean
        @ServiceConnection
        KafkaContainer kafka() {
            return new KafkaContainer("apache/kafka:4.1.0").withStartupTimeout(Duration.ofMinutes(2));
        }

        @Bean
        Probe probe() {
            return new Probe();
        }
    }
}
