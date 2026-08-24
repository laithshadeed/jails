package com.example.paymentsgateway.jobs;

import com.example.paymentsgateway.domain.PaymentMethod;
import java.util.Objects;
import java.util.UUID;

/**
 * Stable, persistable input for the SettlementDispatcher durable job.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record SettlementDispatcherWork(UUID id, UUID merchantId, String idempotencyKey, long amountMinor, String currency, PaymentMethod method) {

    public SettlementDispatcherWork {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(merchantId, "merchantId");
        Objects.requireNonNull(idempotencyKey, "idempotencyKey");
        Objects.requireNonNull(currency, "currency");
        Objects.requireNonNull(method, "method");
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
