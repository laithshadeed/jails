package com.example.paymentsgateway.web;

import com.example.paymentsgateway.domain.Merchant;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Merchant itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record MerchantResponse(
        UUID id,
        String reference,
        String displayName,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code merchant}. */
    public static MerchantResponse from(Merchant merchant) {
        return new MerchantResponse(
                merchant.id(),
                merchant.reference(),
                merchant.displayName(),
                merchant.createdAt(),
                merchant.updatedAt());
    }
}
