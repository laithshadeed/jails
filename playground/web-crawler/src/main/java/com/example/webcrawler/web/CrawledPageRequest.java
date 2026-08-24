package com.example.webcrawler.web;

import com.example.webcrawler.domain.CrawledPage;
import jakarta.validation.constraints.NotNull;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;

/**
 * What a client may send. Deliberately not CrawledPage itself.
 *
 * <p>A domain type used as the wire contract couples the two permanently:
 * renaming a component becomes a breaking API change, and adding one
 * publishes it whether or not that was intended. The cost of keeping them
 * apart is this file; the cost of not doing is paid later and by someone else.
 *
 * <p>The constraints come from the field spec, so a malformed request is
 * rejected before any application code runs. With {@code jails add api} the
 * rejection is reported as a 400 naming each bad field.
 */
public record CrawledPageRequest(
        @NotNull UUID id,
        @NotNull UUID crawlRunId,
        @NotNull URI url,
        @NotNull Integer statusCode,
        @NotNull Instant discoveredAt,
        @NotNull Instant createdAt,
        @NotNull Instant updatedAt) {

    /** @return the domain type this request describes. */
    public CrawledPage toDomain() {
        return new CrawledPage(
                id,
                crawlRunId,
                url,
                statusCode,
                discoveredAt,
                createdAt,
                updatedAt);
    }
}
