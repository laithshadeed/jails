package com.example.demo.domain;

/** Matches transactions made in Great Britain. */
public final class DomesticEligibility implements Eligibility {

    @Override
    public boolean matches(Transaction transaction) {
        return "GB".equalsIgnoreCase(transaction.country());
    }
}
