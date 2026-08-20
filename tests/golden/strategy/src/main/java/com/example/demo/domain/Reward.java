package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Reward value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Reward(UUID id, long amount) {

    public Reward {
        Objects.requireNonNull(id, "id");
    }
}
