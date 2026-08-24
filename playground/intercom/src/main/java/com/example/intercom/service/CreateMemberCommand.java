package com.example.intercom.service;

import com.example.intercom.domain.MemberRole;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the CreateMember use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CreateMemberCommand(UUID id, UUID workspaceId, String email, String displayName, MemberRole role) {

    public CreateMemberCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(email, "email");
        Objects.requireNonNull(displayName, "displayName");
        Objects.requireNonNull(role, "role");
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
        displayName = displayName.trim();
        if (displayName.isEmpty()) {
            throw new IllegalArgumentException("displayName must not be blank");
        }
    }
}
