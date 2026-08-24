package com.example.paymentsgateway.service;

import com.example.paymentsgateway.app.MerchantRepository;
import com.example.paymentsgateway.domain.Merchant;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Merchant}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class MerchantService {

    private final MerchantRepository repository;

    public MerchantService(MerchantRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Merchant> findAll() {
        return repository.findAll();
    }

    public Optional<Merchant> findById(String id) {
        return repository.findById(id);
    }

    public Merchant create(Merchant merchant) {
        repository.save(merchant);
        return merchant;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
