package com.example.demo.service;

import com.example.demo.app.AuthorRepository;
import com.example.demo.domain.Author;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Author}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class AuthorService {

    private final AuthorRepository repository;

    public AuthorService(AuthorRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Author> findAll() {
        return repository.findAll();
    }

    public Optional<Author> findById(Long id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Author create(Author author) {
        return repository.save(author);
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(Long id) {
        return repository.deleteById(id);
    }
}
