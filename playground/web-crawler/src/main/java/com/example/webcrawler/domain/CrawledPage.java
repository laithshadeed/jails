package com.example.webcrawler.domain;

import java.net.URI;
import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * An immutable CrawledPage value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CrawledPage(UUID id, UUID crawlRunId, URI url, int statusCode, Instant discoveredAt, Instant createdAt, Instant updatedAt) {

    public CrawledPage {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(crawlRunId, "crawlRunId");
        Objects.requireNonNull(url, "url");
        Objects.requireNonNull(discoveredAt, "discoveredAt");
        Objects.requireNonNull(createdAt, "createdAt");
        Objects.requireNonNull(updatedAt, "updatedAt");
    }
}
