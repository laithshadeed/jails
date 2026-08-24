package com.example.intercom.service;

import com.example.intercom.domain.ConversationStatus;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the ChangeConversationStatus use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ChangeConversationStatusCommand(UUID id, UUID workspaceId, ConversationStatus status, long version) {

    public ChangeConversationStatusCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(status, "status");
    }
}
