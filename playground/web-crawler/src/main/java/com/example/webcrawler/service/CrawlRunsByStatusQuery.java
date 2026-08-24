package com.example.webcrawler.service;

import com.example.webcrawler.domain.CrawlStatus;
import java.util.Objects;

/**
 * Typed filters for the CrawlRunsByStatus query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record CrawlRunsByStatusQuery(CrawlStatus status) {

    public CrawlRunsByStatusQuery {
        Objects.requireNonNull(status, "status");
    }
}
