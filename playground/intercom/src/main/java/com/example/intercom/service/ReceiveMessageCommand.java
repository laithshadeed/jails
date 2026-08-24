package com.example.intercom.service;

import com.example.intercom.domain.MessageDirection;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the ReceiveMessage use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ReceiveMessageCommand(UUID id, UUID workspaceId, UUID conversationId, MessageDirection direction, String body) {

    public ReceiveMessageCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(conversationId, "conversationId");
        Objects.requireNonNull(direction, "direction");
        Objects.requireNonNull(body, "body");
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
