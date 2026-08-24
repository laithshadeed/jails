package com.example.webcrawler.service;

import com.example.webcrawler.domain.CrawledPage;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface PagesByCrawlRunQueryPort {

    List<CrawledPage> execute(PagesByCrawlRunQuery query);
}
