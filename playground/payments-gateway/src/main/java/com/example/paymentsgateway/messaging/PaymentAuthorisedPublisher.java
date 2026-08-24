package com.example.paymentsgateway.messaging;

import java.util.concurrent.CompletableFuture;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link PaymentAuthorisedEvent}.
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
public class PaymentAuthorisedPublisher {

    private final KafkaTemplate<String, PaymentAuthorisedEvent> kafka;
    private final String topic;

    public PaymentAuthorisedPublisher(
            KafkaTemplate<String, PaymentAuthorisedEvent> kafka,
            @Value("${topics.payment-authorised:payment-authorised}") String topic) {
        this.kafka = kafka;
        this.topic = topic;
    }

    /** The returned acknowledgement lets durable callers mark success only after Kafka accepts it. */
    public CompletableFuture<SendResult<String, PaymentAuthorisedEvent>> publish(PaymentAuthorisedEvent event) {
        return kafka.send(topic, String.valueOf(event.id()), event);
    }
}
