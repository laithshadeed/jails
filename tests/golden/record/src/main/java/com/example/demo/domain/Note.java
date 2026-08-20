package com.example.demo.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.Optional;

/**
 * An immutable Note value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record Note(String title, Optional<String> body, Instant at) {

    public Note {
        Objects.requireNonNull(title, "title");
        Objects.requireNonNull(at, "at");
        body = Objects.requireNonNullElse(body, Optional.empty());
        title = title.trim();
        if (title.isEmpty()) {
            throw new IllegalArgumentException("title must not be blank");
        }
    }
}
