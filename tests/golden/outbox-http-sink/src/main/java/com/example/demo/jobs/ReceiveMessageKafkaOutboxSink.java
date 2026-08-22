package com.example.demo.jobs;

import com.example.demo.messaging.MessageReceivedEvent;
import com.example.demo.messaging.MessageReceivedPublisher;
import org.springframework.core.annotation.Order;
import org.springframework.stereotype.Component;

/** Kafka destination in the same generic sink chain as provider delivery. */
@Component
@Order(0)
public final class ReceiveMessageKafkaOutboxSink implements ReceiveMessageOutboxSink {
    private final MessageReceivedPublisher publisher;

    public ReceiveMessageKafkaOutboxSink(MessageReceivedPublisher publisher) {
        this.publisher = publisher;
    }

    @Override public String name() { return "kafka"; }
    @Override public void deliver(MessageReceivedEvent event) { publisher.publish(event).join(); }
}
