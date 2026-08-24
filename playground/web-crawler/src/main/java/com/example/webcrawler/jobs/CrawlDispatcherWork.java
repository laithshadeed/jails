package com.example.webcrawler.jobs;

import java.net.URI;
import java.util.Objects;
import java.util.UUID;

/**
 * Stable, persistable input for the CrawlDispatcher durable job.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CrawlDispatcherWork(UUID id, URI seedUrl) {

    public CrawlDispatcherWork {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(seedUrl, "seedUrl");
    }
}
