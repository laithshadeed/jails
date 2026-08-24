package com.example.webcrawler.messaging;

import java.net.URI;
import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * Immutable payload published as PageDiscoveredEvent.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PageDiscoveredEvent(UUID id, UUID crawlRunId, URI url, Instant occurredAt) {

    public PageDiscoveredEvent {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(crawlRunId, "crawlRunId");
        Objects.requireNonNull(url, "url");
        Objects.requireNonNull(occurredAt, "occurredAt");
    }
}
