package com.example.intercom.web;

import com.example.intercom.domain.Contact;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Contact itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record ContactResponse(
        UUID id,
        UUID workspaceId,
        String email,
        String displayName,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code contact}. */
    public static ContactResponse from(Contact contact) {
        return new ContactResponse(
                contact.id(),
                contact.workspaceId(),
                contact.email(),
                contact.displayName().orElse(null),
                contact.createdAt(),
                contact.updatedAt());
    }
}
