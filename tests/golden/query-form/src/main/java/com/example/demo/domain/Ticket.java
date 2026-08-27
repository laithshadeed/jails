package com.example.demo.domain;

import java.util.Objects;
import java.util.Optional;

/**
 * An immutable Ticket value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record Ticket(long id, String subject, Optional<String> status) {

    public Ticket {
        Objects.requireNonNull(subject, "subject");
        status = Objects.requireNonNullElse(status, Optional.empty());
        subject = subject.trim();
        if (subject.isEmpty()) {
            throw new IllegalArgumentException("subject must not be blank");
        }
    }
}
