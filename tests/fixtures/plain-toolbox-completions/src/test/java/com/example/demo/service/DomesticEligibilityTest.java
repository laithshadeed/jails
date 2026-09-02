package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Transaction;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class DomesticEligibilityTest {

    @Test
    void matchesWhenTheTransactionQualifies() {
        var rule = new DomesticEligibility();
        var transaction = new Transaction(UUID.randomUUID(), 500L, "GB");

        assertThat(rule.appliesTo(transaction)).isTrue();
    }

    @Test
    void declinesWhenTheTransactionDoesNot() {
        var rule = new DomesticEligibility();
        var transaction = new Transaction(UUID.randomUUID(), 500L, "FR");

        assertThat(rule.appliesTo(transaction)).isFalse();
    }
}
