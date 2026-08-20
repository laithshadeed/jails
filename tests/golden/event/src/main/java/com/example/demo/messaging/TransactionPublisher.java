package com.example.demo.messaging;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link TransactionEvent}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
 * <p>The key is the event id, which is what gives ordering per entity --
 * Kafka only guarantees order within a partition, and a null key round-robins
 * across all of them. Getting this wrong produces a system that works until
 * it has traffic.
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

    /** Publishes asynchronously; the send is in flight when this returns. */
    public void publish(TransactionEvent event) {
        kafka.send(topic, event.id(), event);
    }
}
