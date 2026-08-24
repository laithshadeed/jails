package com.example.webcrawler.messaging;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.stereotype.Component;

/**
 * Consumes {@link PageDiscoveredEvent}.
 *
 * <p>The listener is deliberately thin: it hands the event to the application
 * and does nothing else. Business logic inside a listener is unreachable from
 * any test that does not start a broker, and unreusable from any other entry
 * point.
 *
 * <p>Nothing here catches exceptions. That is the right default -- a thrown
 * exception means the offset is not committed, so the message is retried and
 * eventually goes to a dead-letter topic if one is configured. Swallowing it
 * would acknowledge a message that was never processed, which is data loss
 * that looks like success.
 */
@Component
public class PageDiscoveredListener {

    private static final Logger log = LoggerFactory.getLogger(PageDiscoveredListener.class);

    @KafkaListener(topics = "${topics.page-discovered:page-discovered}")
    public void on(PageDiscoveredEvent event) {
        log.info("received {}", event.id());
        // TODO: hand this to the application service that owns the reaction.
    }
}
