package com.example.demo.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the AddItem use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record AddItemCommand(UUID id, UUID ownerId, String name) {

    public AddItemCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(ownerId, "ownerId");
        Objects.requireNonNull(name, "name");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
