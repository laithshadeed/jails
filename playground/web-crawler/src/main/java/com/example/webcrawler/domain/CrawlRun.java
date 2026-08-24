package com.example.webcrawler.domain;

import java.net.URI;
import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * An immutable CrawlRun value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record CrawlRun(UUID id, URI seedUrl, CrawlStatus status, long pagesVisited, Optional<Instant> startedAt, Optional<Instant> finishedAt, Instant createdAt, Instant updatedAt) {

    public CrawlRun {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(seedUrl, "seedUrl");
        Objects.requireNonNull(status, "status");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
        startedAt = Objects.requireNonNullElse(startedAt, Optional.empty());
        finishedAt = Objects.requireNonNullElse(finishedAt, Optional.empty());
    }
}
