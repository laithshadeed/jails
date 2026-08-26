package com.example.demo.service;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Payout}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class PayoutService {

    private final PayoutRepository repository;

    public PayoutService(PayoutRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Payout> findAll() {
        return repository.findAll();
    }

    public Optional<Payout> findById(UUID id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Payout create(Payout payout) {
        return repository.save(new Payout(
                UUID.randomUUID(),
                payout.amount(),
                payout.status(),
                payout.version(),
                payout.createdAt()));
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
