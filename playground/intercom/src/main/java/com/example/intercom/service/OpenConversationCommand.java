package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the OpenConversation use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record OpenConversationCommand(UUID id, UUID workspaceId, UUID contactId, UUID inboxId) {

    public OpenConversationCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(contactId, "contactId");
        Objects.requireNonNull(inboxId, "inboxId");
    }
}
