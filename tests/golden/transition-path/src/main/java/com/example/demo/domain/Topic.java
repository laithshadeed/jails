package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable Topic value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Topic(long id, long userId, String subject, long version) {

    public Topic {
        Objects.requireNonNull(subject, "subject");
        subject = subject.trim();
        if (subject.isEmpty()) {
            throw new IllegalArgumentException("subject must not be blank");
        }
    }
}
