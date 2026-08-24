package com.example.paymentsgateway.web;

import com.example.paymentsgateway.domain.Refund;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Refund itself.
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
public record RefundRequest(
        @NotNull UUID id,
        @NotNull UUID merchantId,
        @NotNull UUID paymentId,
        @NotNull Long amountMinor,
        String reason) {

    /** @return the domain type this request describes. */
    public Refund toDomain() {
        // Set here, not received: these are audit columns, and one
                 // instant so a freshly created row does not look already edited.
                 Instant now = Instant.now();
        return new Refund(
                id,
                merchantId,
                paymentId,
                amountMinor,
                Optional.ofNullable(reason),
                now,
                now);
    }
}
