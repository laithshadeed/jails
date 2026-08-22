package com.example.demo.jobs;

import java.util.Objects;
import java.util.UUID;

/**
 * Stable, persistable input for the ItemDispatcher durable job.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ItemDispatcherWork(UUID id, UUID ownerId, String name) {

    public ItemDispatcherWork {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(ownerId, "ownerId");
        Objects.requireNonNull(name, "name");
        name = name.trim();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name must not be blank");
        }
    }
}
