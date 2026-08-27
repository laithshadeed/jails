package com.example.demo.web;

import com.example.demo.domain.Owner;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Owner itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record OwnerResponse(
        UUID id,
        String email,
        Instant createdAt) {

    /** @return the response describing {@code owner}. */
    public static OwnerResponse from(Owner owner) {
        return new OwnerResponse(
                owner.id(),
                owner.email(),
                owner.createdAt());
    }
}
