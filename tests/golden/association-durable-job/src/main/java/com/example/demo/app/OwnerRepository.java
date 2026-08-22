package com.example.demo.app;

import com.example.demo.domain.Owner;
import java.util.List;
import java.util.Optional;

/**
 * Storage for {@link Owner}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{@code findById} returns {@link Optional} rather than null, and
 * {@code findAll} an empty list rather than null, so no caller has to guard.
 */
public interface OwnerRepository {

    Optional<Owner> findById(String id);

    List<Owner> findAll();

    /** Inserts a row. Define conflict behavior explicitly in the SQL adapter. */
    void save(Owner owner);

    /** @return true when a row was actually removed. */
    boolean deleteById(String id);
}
