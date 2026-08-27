package com.example.demo.service;

import java.util.Objects;

/**
 * Validated input for the PostNote use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PostNoteCommand(String email, String body) {

    public PostNoteCommand {
        Objects.requireNonNull(email, "email");
        Objects.requireNonNull(body, "body");
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
