package com.example.intercom.service;

import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * Validated input for the CreateContact use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record CreateContactCommand(UUID id, UUID workspaceId, String email, Optional<String> displayName) {

    public CreateContactCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(email, "email");
        displayName = Objects.requireNonNullElse(displayName, Optional.empty());
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
    }
}
