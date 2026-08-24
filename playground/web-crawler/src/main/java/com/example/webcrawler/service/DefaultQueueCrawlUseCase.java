package com.example.webcrawler.service;

import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultQueueCrawlUseCase implements QueueCrawlUseCase {

    private final CrawlRunRepository repository;

    public DefaultQueueCrawlUseCase(CrawlRunRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public CrawlRun execute(QueueCrawlCommand command) {
        Objects.requireNonNull(command, "command is required");
        CrawlRun crawlRun = new CrawlRun(
                command.id(),
                command.seedUrl(),
                CrawlStatus.values()[0],
                0L,
                Optional.empty(),
                Optional.empty(),
                Instant.now(),
                Instant.now());
        repository.save(crawlRun);
        return crawlRun;
    }
}
