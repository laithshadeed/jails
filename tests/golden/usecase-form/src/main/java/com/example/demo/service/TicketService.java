package com.example.demo.service;

import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Ticket}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class TicketService {

    private final TicketRepository repository;

    public TicketService(TicketRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Ticket> findAll() {
        return repository.findAll();
    }

    public Optional<Ticket> findById(Long id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Ticket create(Ticket ticket) {
        return repository.save(ticket);
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(Long id) {
        return repository.deleteById(id);
    }
}
