package com.example.intercom.web;

import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

/**
 * What a client may send. Deliberately not Message itself.
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
public record MessageRequest(
        @NotNull UUID id,
        @NotNull UUID workspaceId,
        @NotNull UUID conversationId,
        @NotNull MessageDirection direction,
        @NotBlank String body,
        @NotNull Instant createdAt,
        @NotNull Instant updatedAt) {

    /** @return the domain type this request describes. */
    public Message toDomain() {
        return new Message(
                id,
                workspaceId,
                conversationId,
                direction,
                body,
                createdAt,
                updatedAt);
    }
}
