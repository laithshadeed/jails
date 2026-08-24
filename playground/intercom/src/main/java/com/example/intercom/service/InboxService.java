package com.example.intercom.service;

import com.example.intercom.app.InboxRepository;
import com.example.intercom.domain.Inbox;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Inbox}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class InboxService {

    private final InboxRepository repository;

    public InboxService(InboxRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Inbox> findAll() {
        return repository.findAll();
    }

    public Optional<Inbox> findById(String id) {
        return repository.findById(id);
    }

    public Inbox create(Inbox inbox) {
        repository.save(inbox);
        return inbox;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
