package com.example.webcrawler.service;

import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link CrawlRun}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class CrawlRunService {

    private final CrawlRunRepository repository;

    public CrawlRunService(CrawlRunRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<CrawlRun> findAll() {
        return repository.findAll();
    }

    public Optional<CrawlRun> findById(String id) {
        return repository.findById(id);
    }

    public CrawlRun create(CrawlRun crawlRun) {
        repository.save(crawlRun);
        return crawlRun;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
