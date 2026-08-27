package com.example.demo.domain;

/**
 * One reason a transaction produces a result.
 *
 * <p>An open set: every implementation is a bean, and Spring collects them
 * into a {@code List<Eligibility>}. Implementations are independent and each one
 * sees every input, so more than one may answer.
 *
 * <p>{@link EligibilityEvaluator} is where the whole set is taken as one
 * constructor parameter, which is what makes adding an implementation a
 * matter of writing the class and nothing else.
 *
 * <p>Evaluation should be pure -- no clock beyond one the implementation was
 * built with, no database, no network -- so the same transaction always
 * yields the same answer.
 */
public interface Eligibility {

    /** Whether this applies to the given transaction. */
    boolean matches(Transaction transaction);
}
