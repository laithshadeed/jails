package com.example.intercom.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable ConversationAssignment value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ConversationAssignment(UUID id, UUID workspaceId, UUID conversationId, UUID memberId, AssignmentStatus status, long version, Instant assignedAt, Instant createdAt, Instant updatedAt) {

    public ConversationAssignment {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(conversationId, "conversationId");
        Objects.requireNonNull(memberId, "memberId");
        Objects.requireNonNull(status, "status");
        Objects.requireNonNull(assignedAt, "assignedAt");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
    }
}
