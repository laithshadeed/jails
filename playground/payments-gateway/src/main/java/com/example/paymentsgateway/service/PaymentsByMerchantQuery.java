package com.example.paymentsgateway.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the PaymentsByMerchant query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PaymentsByMerchantQuery(UUID merchantId) {

    public PaymentsByMerchantQuery {
        Objects.requireNonNull(merchantId, "merchantId");
    }
}
