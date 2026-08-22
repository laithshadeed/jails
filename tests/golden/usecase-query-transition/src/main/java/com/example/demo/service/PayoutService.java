package com.example.demo.service;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
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

    public Optional<Payout> findById(String id) {
        return repository.findById(id);
    }

    public Payout create(Payout payout) {
        repository.save(payout);
        return payout;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
