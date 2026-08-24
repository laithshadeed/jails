package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the AssignConversation use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record AssignConversationCommand(UUID id, UUID workspaceId, UUID conversationId, UUID memberId) {

    public AssignConversationCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(conversationId, "conversationId");
        Objects.requireNonNull(memberId, "memberId");
    }
}
