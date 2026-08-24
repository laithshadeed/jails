package com.example.webcrawler.web;

import com.example.webcrawler.domain.CrawledPage;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not CrawledPage itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record CrawledPageResponse(
        UUID id,
        UUID crawlRunId,
        URI url,
        Integer statusCode,
        Instant discoveredAt,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code crawledPage}. */
    public static CrawledPageResponse from(CrawledPage crawledPage) {
        return new CrawledPageResponse(
                crawledPage.id(),
                crawledPage.crawlRunId(),
                crawledPage.url(),
                crawledPage.statusCode(),
                crawledPage.discoveredAt(),
                crawledPage.createdAt(),
                crawledPage.updatedAt());
    }
}
