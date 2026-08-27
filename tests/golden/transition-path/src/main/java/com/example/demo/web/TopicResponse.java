package com.example.demo.web;

import com.example.demo.domain.Topic;

/**
 * What this application returns. Deliberately not Topic itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record TopicResponse(
        Long id,
        Long userId,
        String subject,
        Long version) {

    /** @return the response describing {@code topic}. */
    public static TopicResponse from(Topic topic) {
        return new TopicResponse(
                topic.id(),
                topic.userId(),
                topic.subject(),
                topic.version());
    }
}
