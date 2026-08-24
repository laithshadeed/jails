package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the AddInboxMember use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record AddInboxMemberCommand(UUID id, UUID workspaceId, UUID inboxId, UUID memberId) {

    public AddInboxMemberCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(inboxId, "inboxId");
        Objects.requireNonNull(memberId, "memberId");
    }
}
