package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable Operator value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Operator(long id, String email) {

    public Operator {
        Objects.requireNonNull(email, "email");
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
    }
}
