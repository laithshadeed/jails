package com.example.paymentsgateway.jobs;

import com.example.paymentsgateway.messaging.PaymentAuthorisedEvent;
import com.example.paymentsgateway.messaging.PaymentAuthorisedPublisher;
import org.springframework.core.annotation.Order;
import org.springframework.stereotype.Component;

/** Kafka destination in the same generic sink chain as provider delivery. */
@Component
@Order(0)
public final class AuthorisePaymentKafkaOutboxSink implements AuthorisePaymentOutboxSink {
    private final PaymentAuthorisedPublisher publisher;

    public AuthorisePaymentKafkaOutboxSink(PaymentAuthorisedPublisher publisher) {
        this.publisher = publisher;
    }

    @Override public String name() { return "kafka"; }
    @Override public void deliver(PaymentAuthorisedEvent event) { publisher.publish(event).join(); }
}
