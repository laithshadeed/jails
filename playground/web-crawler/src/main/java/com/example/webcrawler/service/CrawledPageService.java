package com.example.webcrawler.service;

import com.example.webcrawler.app.CrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link CrawledPage}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class CrawledPageService {

    private final CrawledPageRepository repository;

    public CrawledPageService(CrawledPageRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<CrawledPage> findAll() {
        return repository.findAll();
    }

    public Optional<CrawledPage> findById(String id) {
        return repository.findById(id);
    }

    public CrawledPage create(CrawledPage crawledPage) {
        repository.save(crawledPage);
        return crawledPage;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
