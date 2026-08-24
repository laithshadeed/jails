package com.example.paymentsgateway.service;

import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Payment}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class PaymentService {

    private final PaymentRepository repository;

    public PaymentService(PaymentRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Payment> findAll() {
        return repository.findAll();
    }

    public Optional<Payment> findById(String id) {
        return repository.findById(id);
    }

    public Payment create(Payment payment) {
        repository.save(payment);
        return payment;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
