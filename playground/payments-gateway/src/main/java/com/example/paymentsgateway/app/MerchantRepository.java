package com.example.paymentsgateway.app;

import com.example.paymentsgateway.domain.Merchant;
import java.util.List;
import java.util.Optional;

/**
 * Storage for {@link Merchant}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{@code findById} returns {@link Optional} rather than null, and
 * {@code findAll} an empty list rather than null, so no caller has to guard.
 */
public interface MerchantRepository {

    Optional<Merchant> findById(String id);

    List<Merchant> findAll();

    /** Inserts a row. Define conflict behavior explicitly in the SQL adapter. */
    void save(Merchant merchant);

    /** @return true when a row was actually removed. */
    boolean deleteById(String id);
}
