package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.PaymentStatus;
import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the PaymentsByStatus query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PaymentsByStatusQuery(UUID merchantId, PaymentStatus status) {

    public PaymentsByStatusQuery {
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(status, "status");
    }
}
