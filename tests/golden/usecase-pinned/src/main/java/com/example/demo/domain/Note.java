package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable Note value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Note(long id, long authorId, String body, SenderType senderType) {

    public Note {
        Objects.requireNonNull(body, "body");
        Objects.requireNonNull(senderType, "senderType");
        body = body.trim();
        if (body.isEmpty()) {
            throw new IllegalArgumentException("body must not be blank");
        }
    }
}
