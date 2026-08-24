package com.example.paymentsgateway.web;

import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Payment itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record PaymentResponse(
        UUID id,
        UUID merchantId,
        String idempotencyKey,
        Long amountMinor,
        String currency,
        PaymentMethod method,
        PaymentStatus status,
        Long version,
        Instant authorisedAt,
        Instant capturedAt,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code payment}. */
    public static PaymentResponse from(Payment payment) {
        return new PaymentResponse(
                payment.id(),
                payment.merchantId(),
                payment.idempotencyKey(),
                payment.amountMinor(),
                payment.currency(),
                payment.method(),
                payment.status(),
                payment.version(),
                payment.authorisedAt().orElse(null),
                payment.capturedAt().orElse(null),
                payment.createdAt(),
                payment.updatedAt());
    }
}
