package com.example.demo.service;

import com.example.demo.app.TopicRepository;
import com.example.demo.domain.Topic;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Topic}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class TopicService {

    private final TopicRepository repository;

    public TopicService(TopicRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Topic> findAll() {
        return repository.findAll();
    }

    public Optional<Topic> findById(Long id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Topic create(Topic topic) {
        return repository.save(topic);
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(Long id) {
        return repository.deleteById(id);
    }
}
