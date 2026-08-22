package com.example.demo.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Note value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Note(UUID id, String title, Instant createdAt) {

    public Note {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(title, "title");
        Objects.requireNonNull(createdAt, "createdAt");
        title = title.trim();
        if (title.isEmpty()) {
            throw new IllegalArgumentException("title must not be blank");
        }
    }
}
