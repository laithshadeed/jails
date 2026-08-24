package com.example.paymentsgateway.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the RefundPaymentRequest use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record RefundPaymentRequestCommand(UUID id, UUID merchantId, UUID paymentId, long amountMinor) {

    public RefundPaymentRequestCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(paymentId, "paymentId");
    }
}
