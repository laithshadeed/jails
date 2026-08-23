package com.example.demo.messaging;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.KafkaTestcontainersConfig;
import java.time.Instant;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.kafka.annotation.KafkaListener;

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
@SpringBootTest(properties = {
    "spring.kafka.consumer.properties.group.protocol=classic",
    "spring.kafka.listener.auto-startup=true"
})
@Import({ KafkaTestcontainersConfig.class, TransactionMessagingIT.ProbeConfiguration.class })
class TransactionMessagingIT {

    @Autowired
    private TransactionPublisher publisher;

    @Autowired
    private Probe probe;

    @Test
    void aPublishedEventIsConsumed() throws InterruptedException {
        TransactionEvent event = new TransactionEvent("probe-1", Instant.parse("2024-01-01T00:00:00Z"));

        publisher.publish(event);

        assertThat(probe.received.await(30, TimeUnit.SECONDS))
                .as("the event should have been consumed within 30s")
                .isTrue();
        assertThat(probe.last.get().id()).isEqualTo("probe-1");
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
        private final AtomicReference<TransactionEvent> last = new AtomicReference<>();

        /**
         * Its own consumer group, so it does not compete with the
         * application's listener: two consumers in one group split the
         * partitions and each message reaches only one of them.
         */
        @KafkaListener(topics = "${topics.transaction:transaction}", groupId = "transaction-it-probe")
        void on(TransactionEvent event) {
            last.set(event);
            received.countDown();
        }
    }

    @org.springframework.boot.test.context.TestConfiguration(proxyBeanMethods = false)
    static class ProbeConfiguration {

        @org.springframework.context.annotation.Bean
        Probe probe() {
            return new Probe();
        }
    }
}
