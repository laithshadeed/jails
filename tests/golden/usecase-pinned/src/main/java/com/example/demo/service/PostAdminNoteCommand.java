package com.example.demo.service;

import java.util.Objects;

/**
 * Validated input for the PostAdminNote use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PostAdminNoteCommand(long authorId, String body) {

    public PostAdminNoteCommand {
        Objects.requireNonNull(body, "body");
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
