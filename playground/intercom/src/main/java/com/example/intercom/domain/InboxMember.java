package com.example.intercom.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable InboxMember value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record InboxMember(UUID id, UUID workspaceId, UUID inboxId, UUID memberId, Instant createdAt, Instant updatedAt) {

    public InboxMember {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(workspaceId, "workspaceId");
        Objects.requireNonNull(inboxId, "inboxId");
        Objects.requireNonNull(memberId, "memberId");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
    }
}
