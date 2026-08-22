package com.example.demo.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Owner value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Owner(UUID id, String name, Instant createdAt) {

    public Owner {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(name, "name");
        Objects.requireNonNull(createdAt, "createdAt");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
