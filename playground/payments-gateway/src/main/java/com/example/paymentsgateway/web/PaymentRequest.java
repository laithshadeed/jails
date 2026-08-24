package com.example.paymentsgateway.web;

import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Payment itself.
 *
 * <p>A domain type used as the wire contract couples the two permanently:
 * renaming a component becomes a breaking API change, and adding one
 * publishes it whether or not that was intended. The cost of keeping them
 * apart is this file; the cost of not doing is paid later and by someone else.
 *
 * <p>The constraints come from the field spec, so a malformed request is
 * rejected before any application code runs. With {@code jails add api} the
 * rejection is reported as a 400 naming each bad field.
 */
public record PaymentRequest(
        @NotNull UUID id,
        @NotNull UUID merchantId,
        @NotBlank String idempotencyKey,
        @NotNull Long amountMinor,
        @NotBlank String currency,
        @NotNull PaymentMethod method,
        @NotNull PaymentStatus status,
        @NotNull Long version,
        Instant authorisedAt,
        Instant capturedAt) {

    /** @return the domain type this request describes. */
    public Payment toDomain() {
        // Audit columns: set here rather than received, and one
        // instant for both, so a freshly created row does not look
        // already edited.
        Instant now = Instant.now();
        return new Payment(
                id,
                merchantId,
                idempotencyKey,
                amountMinor,
                currency,
                method,
                status,
                version,
                Optional.ofNullable(authorisedAt),
                Optional.ofNullable(capturedAt),
                now,
                now);
    }
}
