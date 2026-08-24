package com.example.webcrawler.service;

import com.example.webcrawler.domain.CrawlRun;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface QueueCrawlUseCase {

    CrawlRun execute(QueueCrawlCommand command);
}
