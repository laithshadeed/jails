package com.example.webcrawler.web;

import com.example.webcrawler.service.QueueCrawlCommand;
import com.example.webcrawler.service.QueueCrawlUseCase;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping(QueueCrawlController.PATH)
public final class QueueCrawlController {

    public static final String PATH = "/actions/queue-crawl";
    private static final String RESOURCE_PATH = "/crawl-runs";

    private final QueueCrawlUseCase useCase;

    public QueueCrawlController(QueueCrawlUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<CrawlRunResponse> execute(
            @Valid @RequestBody QueueCrawlCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(CrawlRunResponse.from(created));
    }
}
