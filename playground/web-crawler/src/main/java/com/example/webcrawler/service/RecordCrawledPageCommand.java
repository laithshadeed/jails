package com.example.webcrawler.service;

import java.net.URI;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the RecordCrawledPage use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record RecordCrawledPageCommand(UUID id, UUID crawlRunId, URI url, int statusCode) {

    public RecordCrawledPageCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(crawlRunId, "crawlRunId");
        Objects.requireNonNull(url, "url");
    }
}
