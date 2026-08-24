package com.example.intercom.web;

import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Member itself.
 *
 * <p>A domain type used as the wire contract couples the two permanently:
 * renaming a component becomes a breaking API change, and adding one
 * publishes it whether or not that was intended. The cost of keeping them
 * apart is this file; the cost of not doing is paid later and by someone else.
 *
 * <p>The constraints come from the field spec, so a malformed request is
 * rejected before any application code runs. With {@code jails add api} the
 * rejection is reported as a 400 naming each bad field.
 */
public record MemberRequest(
        @NotNull UUID id,
        @NotNull UUID workspaceId,
        @NotBlank String email,
        @NotBlank String displayName,
        @NotNull MemberRole role) {

    /** @return the domain type this request describes. */
    public Member toDomain() {
        // Audit columns: set here rather than received, and one
        // instant for both, so a freshly created row does not look
        // already edited.
        Instant now = Instant.now();
        return new Member(
                id,
                workspaceId,
                email,
                displayName,
                role,
                now,
                now);
    }
}
