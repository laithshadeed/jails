package com.example.demo.service;

import com.example.demo.domain.Eligibility;
import com.example.demo.domain.Transaction;

/** Matches transactions made in Great Britain. */
public final class DomesticEligibility implements Eligibility {

    @Override
    public boolean appliesTo(Transaction transaction) {
        return "GB".equalsIgnoreCase(transaction.country());
    }
}
