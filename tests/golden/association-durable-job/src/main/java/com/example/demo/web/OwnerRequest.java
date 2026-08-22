package com.example.demo.web;

import com.example.demo.domain.Owner;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Owner itself.
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
public record OwnerRequest(
        @NotNull UUID id,
        @NotBlank String name,
        @NotNull Instant createdAt) {

    /** @return the domain type this request describes. */
    public Owner toDomain() {
        return new Owner(
                id,
                name,
                createdAt);
    }
}
