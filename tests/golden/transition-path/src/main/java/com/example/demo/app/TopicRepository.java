package com.example.demo.app;

import com.example.demo.domain.Topic;
import java.util.List;
import java.util.Optional;

/**
 * Storage for {@link Topic}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{@code findById} returns {@link Optional} rather than null, and
 * {@code findAll} an empty list rather than null, so no caller has to guard.
 */
public interface TopicRepository {

    Optional<Topic> findById(Long id);

    List<Topic> findAll();

    /**
     * Inserts a row and returns it as stored. Define conflict behavior
     * explicitly in the SQL adapter.
     *
     * <p>The return value is not the argument. The database assigns this table's key, so the returned value 
     * carries it and the argument does not.
     */
    Topic save(Topic topic);

    /** @return true when a row was actually removed. */
    boolean deleteById(Long id);
}
