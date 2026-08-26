package com.example.demo.messaging;

import java.util.concurrent.CompletableFuture;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link MessageReceivedEvent}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
 * <p>The key is messageId, so every record about one Message lands on one
 * partition and Kafka's per-partition order is that Message's order. The
 * component is required for that reason: a null key round-robins across all
 * of them. Getting this wrong produces a system that works until it has
 * traffic.
 */
@Component
public class MessageReceivedPublisher {

    private final KafkaTemplate<String, MessageReceivedEvent> kafka;
    private final String topic;

    public MessageReceivedPublisher(
            KafkaTemplate<String, MessageReceivedEvent> kafka,
            @Value("${topics.message-received:message-received}") String topic) {
        this.kafka = kafka;
        this.topic = topic;
    }

    /** The returned acknowledgement lets durable callers mark success only after Kafka accepts it. */
    public CompletableFuture<SendResult<String, MessageReceivedEvent>> publish(MessageReceivedEvent event) {
        return kafka.send(topic, String.valueOf(event.messageId()), event);
    }
}
