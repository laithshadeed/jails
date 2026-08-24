package com.example.intercom.web;

import com.example.intercom.domain.Contact;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Contact itself.
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
public record ContactRequest(
        @NotNull UUID id,
        @NotNull UUID workspaceId,
        @NotBlank String email,
        String displayName,
        @NotNull Instant createdAt,
        @NotNull Instant updatedAt) {

    /** @return the domain type this request describes. */
    public Contact toDomain() {
        return new Contact(
                id,
                workspaceId,
                email,
                Optional.ofNullable(displayName),
                createdAt,
                updatedAt);
    }
}
