package com.example.paymentsgateway.web;

import com.example.paymentsgateway.domain.Refund;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Refund itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record RefundResponse(
        UUID id,
        UUID merchantId,
        UUID paymentId,
        Long amountMinor,
        String reason,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code refund}. */
    public static RefundResponse from(Refund refund) {
        return new RefundResponse(
                refund.id(),
                refund.merchantId(),
                refund.paymentId(),
                refund.amountMinor(),
                refund.reason().orElse(null),
                refund.createdAt(),
                refund.updatedAt());
    }
}
