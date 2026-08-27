package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable Ticket value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Ticket(long id, String subject) {

    public Ticket {
        Objects.requireNonNull(subject, "subject");
        subject = subject.trim();
        if (subject.isEmpty()) {
            throw new IllegalArgumentException("subject must not be blank");
        }
    }
}
