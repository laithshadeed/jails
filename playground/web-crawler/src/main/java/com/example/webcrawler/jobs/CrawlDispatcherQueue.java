package com.example.webcrawler.jobs;

import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/** Application-facing durable work queue. Reusing an id requires equal payload. */
public interface CrawlDispatcherQueue {

    void enqueue(CrawlDispatcherWork work);

    Optional<Status> status(UUID id);

    enum State { PENDING, RUNNING, SUCCEEDED, FAILED }

    record Status(UUID id, State state, int attempts, Instant nextAttemptAt,
                  Optional<String> lastError, Optional<Instant> completedAt) {}

    final class IdempotencyConflictException extends RuntimeException {
        public IdempotencyConflictException(UUID id) {
            super("work id " + id + " was already used with a different payload");
        }
    }
}
