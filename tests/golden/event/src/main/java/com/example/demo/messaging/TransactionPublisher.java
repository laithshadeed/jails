package com.example.demo.messaging;

import java.util.concurrent.CompletableFuture;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link TransactionEvent}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
 * <p>The key is the event id, which is unique per record -- so records spread
 * across every partition and two events about the same entity have no order
 * between them. Kafka only guarantees order within a partition.
 *
 * <p>If this topic needs per-entity order, carry that entity's id as a
 * component and regenerate with `jails g event Transaction --on <Entity>`.
 */
@Component
public class TransactionPublisher {

    private final KafkaTemplate<String, TransactionEvent> kafka;
    private final String topic;

    public TransactionPublisher(
            KafkaTemplate<String, TransactionEvent> kafka,
            @Value("${topics.transaction:transaction}") String topic) {
        this.kafka = kafka;
        this.topic = topic;
    }

    /** The returned acknowledgement lets durable callers mark success only after Kafka accepts it. */
    public CompletableFuture<SendResult<String, TransactionEvent>> publish(TransactionEvent event) {
        return kafka.send(topic, event.id(), event);
    }
}
