package com.example.demo.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Message value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Message(UUID id, String body, Instant createdAt) {

    public Message {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(body, "body");
        Objects.requireNonNull(createdAt, "createdAt");
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
