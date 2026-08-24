package com.example.intercom.web;

import com.example.intercom.domain.Workspace;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Workspace itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record WorkspaceResponse(
        UUID id,
        String name,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code workspace}. */
    public static WorkspaceResponse from(Workspace workspace) {
        return new WorkspaceResponse(
                workspace.id(),
                workspace.name(),
                workspace.createdAt(),
                workspace.updatedAt());
    }
}
