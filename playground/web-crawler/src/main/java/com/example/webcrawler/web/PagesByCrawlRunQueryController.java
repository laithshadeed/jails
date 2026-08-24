package com.example.webcrawler.web;

import com.example.webcrawler.service.PagesByCrawlRunQuery;
import com.example.webcrawler.service.PagesByCrawlRunQueryPort;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(PagesByCrawlRunQueryController.PATH)
public final class PagesByCrawlRunQueryController {

    public static final String PATH = "/queries/pages-by-crawl-run";

    private final PagesByCrawlRunQueryPort queryPort;

    public PagesByCrawlRunQueryController(PagesByCrawlRunQueryPort queryPort) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");

    }

    @PostMapping
    public List<CrawledPageResponse> execute(
            @Valid @RequestBody PagesByCrawlRunQuery query) {

        return queryPort.execute(query).stream().map(CrawledPageResponse::from).toList();
    }
}
