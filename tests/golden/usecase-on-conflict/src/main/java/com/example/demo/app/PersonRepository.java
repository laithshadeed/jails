package com.example.demo.app;

import com.example.demo.domain.Person;
import java.util.List;
import java.util.Optional;
import java.util.UUID;

/**
 * Storage for {@link Person}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{@code findById} returns {@link Optional} rather than null, and
 * {@code findAll} an empty list rather than null, so no caller has to guard.
 */
public interface PersonRepository {

    Optional<Person> findById(UUID id);

    List<Person> findAll();

    /**
     * Inserts a row and returns it as stored. Define conflict behavior
     * explicitly in the SQL adapter.
     *
     * <p>The return value is not the argument. The application assigns this table's key, so the two are equal 
     * today; returning the stored row is what keeps a caller correct if 
     * that ever stops being true.
     */
    Person save(Person person);

    /** @return true when a row was actually removed. */
    boolean deleteById(UUID id);
}
