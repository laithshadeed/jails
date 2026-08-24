package com.example.intercom.web;

import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Member itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record MemberResponse(
        UUID id,
        UUID workspaceId,
        String email,
        String displayName,
        MemberRole role,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code member}. */
    public static MemberResponse from(Member member) {
        return new MemberResponse(
                member.id(),
                member.workspaceId(),
                member.email(),
                member.displayName(),
                member.role(),
                member.createdAt(),
                member.updatedAt());
    }
}
