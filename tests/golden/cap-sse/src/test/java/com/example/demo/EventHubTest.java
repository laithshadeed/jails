package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

/**
 * What can be tested without a container, which is the part that goes wrong.
 *
 * <p>The delivery path needs a real request, so it belongs in an integration
 * test. The registry does not, and the registry is where the concurrency bug
 * lives: `onCompletion` runs on a container thread while a broadcast is in
 * flight, and the naive `HashMap`/`HashSet` version corrupts under exactly
 * that.
 */
class EventHubTest {

    private final EventHub hub = new EventHub();

    @Test
    void a_subscriber_is_registered_and_unsubscribing_removes_it() {
        SseEmitter emitter = hub.subscribe("orders");
        assertThat(hub.openConnections()).isEqualTo(1);

        // Not `emitter.complete()`. That sets a flag and forwards to a handler
        // the container installs when it takes the emitter, so outside a real
        // request the completion callbacks never run -- which is exactly why
        // `unsubscribe` is public.
        hub.unsubscribe("orders", emitter);

        assertThat(hub.openConnections()).isZero();
    }

    @Test
    void topics_are_separate_so_one_stream_does_not_see_another() {
        hub.subscribe("orders");
        hub.subscribe("shipments");

        assertThat(hub.openConnections()).isEqualTo(2);
    }

    /**
     * The reason the registry is concurrent. This fails on a
     * {@code HashMap}/{@code HashSet} version -- usually as a lost update or a
     * {@code ConcurrentModificationException}, and not on every run, which is
     * the worst kind of failure to meet in production.
     */
    @Test
    void subscribing_and_unsubscribing_at_once_leaves_a_consistent_count()
            throws InterruptedException {
        int threads = 16;
        CountDownLatch start = new CountDownLatch(1);
        CountDownLatch done = new CountDownLatch(threads);

        for (int i = 0; i < threads; i++) {
            boolean completes = i % 2 == 0;
            Thread.ofVirtual()
                    .start(
                            () -> {
                                try {
                                    start.await();
                                    SseEmitter emitter = hub.subscribe("orders");
                                    if (completes) {
                                        hub.unsubscribe("orders", emitter);
                                    }
                                } catch (InterruptedException interrupted) {
                                    Thread.currentThread().interrupt();
                                } finally {
                                    done.countDown();
                                }
                            });
        }
        start.countDown();
        assertThat(done.await(10, TimeUnit.SECONDS)).isTrue();

        assertThat(hub.openConnections()).isEqualTo(threads / 2);
    }

    @Test
    void publishing_to_a_topic_nobody_is_on_is_not_an_error() {
        hub.publish("orders", "created", "{}");

        assertThat(hub.openConnections()).isZero();
    }
}
