package com.example.demo.service;

import java.util.Objects;

/**
 * Validated input for the SetTopicSubject use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record SetTopicSubjectCommand(String subject) {

    public SetTopicSubjectCommand {
        Objects.requireNonNull(subject, "subject");
        subject = subject.trim();
        if (subject.isEmpty()) {
            throw new IllegalArgumentException("subject must not be blank");
        }
    }
}
