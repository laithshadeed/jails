package com.example.intercom.web;

import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Inbox itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record InboxResponse(
        UUID id,
        UUID workspaceId,
        String name,
        InboxChannel channel,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code inbox}. */
    public static InboxResponse from(Inbox inbox) {
        return new InboxResponse(
                inbox.id(),
                inbox.workspaceId(),
                inbox.name(),
                inbox.channel(),
                inbox.createdAt(),
                inbox.updatedAt());
    }
}
