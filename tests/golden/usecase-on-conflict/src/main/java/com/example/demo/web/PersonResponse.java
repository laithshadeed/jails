package com.example.demo.web;

import com.example.demo.domain.Person;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Person itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record PersonResponse(
        UUID id,
        String email,
        Instant createdAt) {

    /** @return the response describing {@code person}. */
    public static PersonResponse from(Person person) {
        return new PersonResponse(
                person.id(),
                person.email(),
                person.createdAt());
    }
}
