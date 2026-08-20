package com.example.demo.domain;


/**
 * TODO: say what makes a transaction qualify under DomesticEligibility.
 */
public final class DomesticEligibility implements Eligibility {

    @Override
    public boolean matches(Transaction transaction) {
        // TODO: decide whether transaction qualifies.
        return false;
    }
}
