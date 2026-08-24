package com.example.intercom.service;

import com.example.intercom.domain.InboxChannel;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the CreateInbox use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CreateInboxCommand(UUID id, UUID workspaceId, String name, InboxChannel channel) {

    public CreateInboxCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(name, "name");
        Objects.requireNonNull(channel, "channel");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
