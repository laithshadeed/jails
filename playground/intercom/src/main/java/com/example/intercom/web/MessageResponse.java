package com.example.intercom.web;

import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Message itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record MessageResponse(
        UUID id,
        UUID workspaceId,
        UUID conversationId,
        MessageDirection direction,
        String body,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code message}. */
    public static MessageResponse from(Message message) {
        return new MessageResponse(
                message.id(),
                message.workspaceId(),
                message.conversationId(),
                message.direction(),
                message.body(),
                message.createdAt(),
                message.updatedAt());
    }
}
