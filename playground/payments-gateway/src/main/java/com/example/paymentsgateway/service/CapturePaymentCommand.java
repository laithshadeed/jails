package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.PaymentStatus;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the CapturePayment use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CapturePaymentCommand(UUID id, UUID merchantId, PaymentStatus status, long version) {

    public CapturePaymentCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(status, "status");
    }
}
