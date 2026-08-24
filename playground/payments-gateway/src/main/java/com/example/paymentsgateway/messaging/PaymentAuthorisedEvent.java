package com.example.paymentsgateway.messaging;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * Immutable payload published as PaymentAuthorisedEvent.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PaymentAuthorisedEvent(UUID id, UUID merchantId, UUID paymentId, Instant occurredAt) {

    public PaymentAuthorisedEvent {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(paymentId, "paymentId");
        Objects.requireNonNull(occurredAt, "occurredAt");
    }
}
