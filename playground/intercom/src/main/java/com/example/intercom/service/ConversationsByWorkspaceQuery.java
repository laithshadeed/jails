package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the ConversationsByWorkspace query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ConversationsByWorkspaceQuery(UUID workspaceId) {

    public ConversationsByWorkspaceQuery {
        Objects.requireNonNull(workspaceId, "workspaceId");
    }
}
