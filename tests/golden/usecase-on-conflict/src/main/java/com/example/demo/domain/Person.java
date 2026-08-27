package com.example.demo.domain;

import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Person value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Person(UUID id, String email, Instant createdAt) {

    public Person {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(email, "email");
        Objects.requireNonNull(createdAt, "createdAt");
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
    }
}
