package com.example.intercom.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Conversation value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Conversation(UUID id, UUID workspaceId, UUID contactId, UUID inboxId, ConversationStatus status, Instant lastMessageAt, long version, Instant createdAt, Instant updatedAt) {

    public Conversation {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(contactId, "contactId");
        Objects.requireNonNull(inboxId, "inboxId");
        Objects.requireNonNull(status, "status");
        Objects.requireNonNull(lastMessageAt, "lastMessageAt");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
    }
}
