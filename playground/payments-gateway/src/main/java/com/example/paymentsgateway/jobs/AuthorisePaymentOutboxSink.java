package com.example.paymentsgateway.jobs;

import com.example.paymentsgateway.messaging.PaymentAuthorisedEvent;

/** One independently configurable destination for a staged event. */
public interface AuthorisePaymentOutboxSink {
    String name();
    void deliver(PaymentAuthorisedEvent event);
}
