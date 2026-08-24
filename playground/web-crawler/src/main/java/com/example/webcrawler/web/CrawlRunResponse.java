package com.example.webcrawler.web;

import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not CrawlRun itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record CrawlRunResponse(
        UUID id,
        URI seedUrl,
        CrawlStatus status,
        Long pagesVisited,
        Instant startedAt,
        Instant finishedAt,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code crawlRun}. */
    public static CrawlRunResponse from(CrawlRun crawlRun) {
        return new CrawlRunResponse(
                crawlRun.id(),
                crawlRun.seedUrl(),
                crawlRun.status(),
                crawlRun.pagesVisited(),
                crawlRun.startedAt().orElse(null),
                crawlRun.finishedAt().orElse(null),
                crawlRun.createdAt(),
                crawlRun.updatedAt());
    }
}
