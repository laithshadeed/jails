package com.example.webcrawler.service;

import com.example.webcrawler.domain.CrawledPage;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface RecordCrawledPageUseCase {

    CrawledPage execute(RecordCrawledPageCommand command);
}
