package com.example.intercom.web;

import com.example.intercom.domain.InboxMember;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not InboxMember itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record InboxMemberResponse(
        UUID id,
        UUID workspaceId,
        UUID inboxId,
        UUID memberId,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code inboxMember}. */
    public static InboxMemberResponse from(InboxMember inboxMember) {
        return new InboxMemberResponse(
                inboxMember.id(),
                inboxMember.workspaceId(),
                inboxMember.inboxId(),
                inboxMember.memberId(),
                inboxMember.createdAt(),
                inboxMember.updatedAt());
    }
}
