package com.example.demo.web;

import com.example.demo.domain.Payout;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Payout itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record PayoutResponse(
        UUID id,
        long amount) {

    /** @return the response describing {@code payout}. */
    public static PayoutResponse from(Payout payout) {
        return new PayoutResponse(
                payout.id(),
                payout.amount());
    }
}
