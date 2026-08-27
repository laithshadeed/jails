package com.example.demo.web;

import com.example.demo.domain.Author;

/**
 * What this application returns. Deliberately not Author itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record AuthorResponse(
        Long id,
        String email) {

    /** @return the response describing {@code author}. */
    public static AuthorResponse from(Author author) {
        return new AuthorResponse(
                author.id(),
                author.email());
    }
}
