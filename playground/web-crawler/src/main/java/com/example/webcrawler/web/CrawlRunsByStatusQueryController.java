package com.example.webcrawler.web;

import com.example.webcrawler.service.CrawlRunsByStatusQuery;
import com.example.webcrawler.service.CrawlRunsByStatusQueryPort;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(CrawlRunsByStatusQueryController.PATH)
public final class CrawlRunsByStatusQueryController {

    public static final String PATH = "/queries/crawl-runs-by-status";

    private final CrawlRunsByStatusQueryPort queryPort;

    public CrawlRunsByStatusQueryController(CrawlRunsByStatusQueryPort queryPort) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");

    }

    @PostMapping
    public List<CrawlRunResponse> execute(
            @Valid @RequestBody CrawlRunsByStatusQuery query) {

        return queryPort.execute(query).stream().map(CrawlRunResponse::from).toList();
    }
}
