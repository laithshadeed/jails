package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the InboxesByWorkspace query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record InboxesByWorkspaceQuery(UUID workspaceId) {

    public InboxesByWorkspaceQuery {
        Objects.requireNonNull(workspaceId, "workspaceId");
    }
}
