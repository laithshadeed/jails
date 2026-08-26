package com.example.demo.web;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Payout itself.
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
public record PayoutRequest(
        @NotNull Long amount,
        @NotNull PayoutStatus status,
        @NotNull Instant createdAt) {

    /**
     * A value nothing reads. The key is assigned after this record is
     * built -- by the service, or by the insert -- and a record component
     * cannot be absent, so the slot has to hold something recognisable.
     */
    private static final UUID PLACEHOLDER_ID = new UUID(0L, 0L);

    /** @return the domain type this request describes. */
    public Payout toDomain() {
        return new Payout(
                PLACEHOLDER_ID,
                amount,
                status,
                0L,
                createdAt);
    }
}
