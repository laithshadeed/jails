package com.example.demo.web;

import com.example.demo.domain.Item;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Item itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record ItemResponse(
        UUID id,
        UUID ownerId,
        String name,
        Instant createdAt) {

    /** @return the response describing {@code item}. */
    public static ItemResponse from(Item item) {
        return new ItemResponse(
                item.id(),
                item.ownerId(),
                item.name(),
                item.createdAt());
    }
}
