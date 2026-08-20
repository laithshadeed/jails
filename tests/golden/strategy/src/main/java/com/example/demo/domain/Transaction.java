package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Transaction value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Transaction(UUID id, long amount) {

    public Transaction {
        Objects.requireNonNull(id, "id");
    }
}
