package com.example.paymentsgateway.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * An immutable Refund value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record Refund(UUID id, UUID merchantId, UUID paymentId, long amountMinor, Optional<String> reason, Instant createdAt, Instant updatedAt) {

    public Refund {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(paymentId, "paymentId");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
        reason = Objects.requireNonNullElse(reason, Optional.empty());
    }
}
