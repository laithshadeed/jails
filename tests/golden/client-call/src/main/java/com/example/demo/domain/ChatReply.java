package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable ChatReply value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ChatReply(String id, String text) {

    public ChatReply {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(text, "text");
        id = id.trim();
        if (id.isEmpty()) {
            throw new IllegalArgumentException("id must not be blank");
        }
        text = text.trim();
        if (text.isEmpty()) {
            throw new IllegalArgumentException("text must not be blank");
        }
    }
}
