package com.example.webcrawler.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Typed filters for the PagesByCrawlRun query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PagesByCrawlRunQuery(UUID crawlRunId) {

    public PagesByCrawlRunQuery {
        Objects.requireNonNull(crawlRunId, "crawlRunId");
    }
}
