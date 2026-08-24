package com.example.webcrawler.web;

import com.example.webcrawler.service.RecordCrawledPageCommand;
import com.example.webcrawler.service.RecordCrawledPageUseCase;
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
@RequestMapping(RecordCrawledPageController.PATH)
public final class RecordCrawledPageController {

    public static final String PATH = "/actions/record-crawled-page";
    private static final String RESOURCE_PATH = "/crawled-pages";

    private final RecordCrawledPageUseCase useCase;

    public RecordCrawledPageController(RecordCrawledPageUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<CrawledPageResponse> execute(
            @Valid @RequestBody RecordCrawledPageCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(CrawledPageResponse.from(created));
    }
}
