package com.example.demo.service;

import com.example.demo.domain.Eligibility;
import com.example.demo.domain.Transaction;
import java.util.List;

/**
 * Every Eligibility, asked about the same transaction in one place.
 *
 * <p>The whole set arrives as one constructor parameter, in the caller's
 * order -- which decides the answer: a rule that responds to everything has
 * to come last, or nothing after it is ever reached.
 */
public final class EligibilityEvaluator {

    private final List<Eligibility> eligibilities;

    public EligibilityEvaluator(List<Eligibility> eligibilities) {
        this.eligibilities = List.copyOf(eligibilities);
    }

    /** Whether any Eligibility matches. */
    public boolean anyMatch(Transaction transaction) {
        return eligibilities.stream().anyMatch(rule -> rule.matches(transaction));
    }

    /** Every Eligibility that matches, in order. */
    public List<Eligibility> matching(Transaction transaction) {
        return eligibilities.stream().filter(rule -> rule.matches(transaction)).toList();
    }
}
