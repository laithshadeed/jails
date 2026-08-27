package com.example.demo.service;

import java.util.Objects;

/**
 * Validated input for the RegisterPerson use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record RegisterPersonCommand(String email) {

    public RegisterPersonCommand {
        Objects.requireNonNull(email, "email");
        email = email.trim();
        if (email.isEmpty()) {
            throw new IllegalArgumentException("email must not be blank");
        }
    }
}
