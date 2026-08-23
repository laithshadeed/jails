package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Article value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Article(UUID id, String title, String body) {

    public Article {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(title, "title");
        Objects.requireNonNull(body, "body");
        title = title.trim();
        if (title.isEmpty()) {
            throw new IllegalArgumentException("title must not be blank");
        }
    }
}
