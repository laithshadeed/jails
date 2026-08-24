package com.example.paymentsgateway.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * An immutable Payment value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record Payment(UUID id, UUID merchantId, String idempotencyKey, long amountMinor, String currency, PaymentMethod method, PaymentStatus status, long version, Optional<Instant> authorisedAt, Optional<Instant> capturedAt, Instant createdAt, Instant updatedAt) {

    public Payment {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(idempotencyKey, "idempotencyKey");
        Objects.requireNonNull(currency, "currency");
        Objects.requireNonNull(method, "method");
        Objects.requireNonNull(status, "status");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
        authorisedAt = Objects.requireNonNullElse(authorisedAt, Optional.empty());
        capturedAt = Objects.requireNonNullElse(capturedAt, Optional.empty());
        idempotencyKey = idempotencyKey.trim();
        if (idempotencyKey.isEmpty()) {
            throw new IllegalArgumentException("idempotencyKey must not be blank");
        }
        currency = currency.trim();
        if (currency.isEmpty()) {
            throw new IllegalArgumentException("currency must not be blank");
        }
    }
}
