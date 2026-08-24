package com.example.intercom.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the CreateWorkspace use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CreateWorkspaceCommand(UUID id, String name) {

    public CreateWorkspaceCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(name, "name");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
