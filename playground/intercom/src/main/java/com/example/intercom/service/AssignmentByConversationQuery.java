package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the AssignmentByConversation query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record AssignmentByConversationQuery(UUID workspaceId, UUID conversationId) {

    public AssignmentByConversationQuery {
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(conversationId, "conversationId");
    }
}
