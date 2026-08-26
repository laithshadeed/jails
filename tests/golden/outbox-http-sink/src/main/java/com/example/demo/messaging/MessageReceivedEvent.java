package com.example.demo.messaging;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * Immutable payload published as MessageReceivedEvent.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record MessageReceivedEvent(UUID id, UUID messageId, Instant occurredAt) {

    public MessageReceivedEvent {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(messageId, "messageId");
        Objects.requireNonNull(occurredAt, "occurredAt");
    }
}
