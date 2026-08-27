package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Widget value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Widget(UUID id, String name) {

    public Widget {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(name, "name");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
