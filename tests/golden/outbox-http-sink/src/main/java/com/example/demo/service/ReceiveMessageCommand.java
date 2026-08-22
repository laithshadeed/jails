package com.example.demo.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the ReceiveMessage use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ReceiveMessageCommand(UUID id, String body) {

    public ReceiveMessageCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(body, "body");
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
