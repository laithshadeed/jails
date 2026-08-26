package com.example.demo.service;

import com.example.demo.app.OwnerRepository;
import com.example.demo.domain.Owner;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Owner}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class OwnerService {

    private final OwnerRepository repository;

    public OwnerService(OwnerRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Owner> findAll() {
        return repository.findAll();
    }

    public Optional<Owner> findById(UUID id) {
        return repository.findById(id);
    }

    public Owner create(Owner owner) {
        repository.save(owner);
        return owner;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
