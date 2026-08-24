package com.example.webcrawler.service;

import com.example.webcrawler.app.CrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultRecordCrawledPageUseCase implements RecordCrawledPageUseCase {

    private final CrawledPageRepository repository;

    public DefaultRecordCrawledPageUseCase(CrawledPageRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public CrawledPage execute(RecordCrawledPageCommand command) {
        Objects.requireNonNull(command, "command is required");
        CrawledPage crawledPage = new CrawledPage(
                command.id(),
                command.crawlRunId(),
                command.url(),
                command.statusCode(),
                Instant.now(),
                Instant.now(),
                Instant.now());
        repository.save(crawledPage);
        return crawledPage;
    }
}
