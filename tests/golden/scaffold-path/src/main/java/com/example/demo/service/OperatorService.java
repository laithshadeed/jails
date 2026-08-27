package com.example.demo.service;

import com.example.demo.app.OperatorRepository;
import com.example.demo.domain.Operator;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Operator}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class OperatorService {

    private final OperatorRepository repository;

    public OperatorService(OperatorRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Operator> findAll() {
        return repository.findAll();
    }

    public Optional<Operator> findById(Long id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Operator create(Operator operator) {
        return repository.save(operator);
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(Long id) {
        return repository.deleteById(id);
    }
}
