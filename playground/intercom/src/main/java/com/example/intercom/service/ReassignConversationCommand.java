package com.example.intercom.service;

import com.example.intercom.domain.AssignmentStatus;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the ReassignConversation use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ReassignConversationCommand(UUID id, UUID workspaceId, UUID memberId, AssignmentStatus status, long version) {

    public ReassignConversationCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(memberId, "memberId");
        Objects.requireNonNull(status, "status");
    }
}
