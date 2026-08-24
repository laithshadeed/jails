package com.example.paymentsgateway.service;

import com.example.paymentsgateway.app.RefundRepository;
import com.example.paymentsgateway.domain.Refund;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Refund}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class RefundService {

    private final RefundRepository repository;

    public RefundService(RefundRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Refund> findAll() {
        return repository.findAll();
    }

    public Optional<Refund> findById(String id) {
        return repository.findById(id);
    }

    public Refund create(Refund refund) {
        repository.save(refund);
        return refund;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
