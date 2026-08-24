package com.example.webcrawler.service;

import com.example.webcrawler.domain.CrawlRun;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface CrawlRunsByStatusQueryPort {

    List<CrawlRun> execute(CrawlRunsByStatusQuery query);
}
