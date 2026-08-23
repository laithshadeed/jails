package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.UUID;
import org.junit.jupiter.api.Test;

class DomesticEligibilityTest {

    @Test
    void matchesWhenTheTransactionQualifies() {
        var rule = new DomesticEligibility();
        var transaction = new Transaction(UUID.randomUUID(), 500L, "GB");

        assertThat(rule.matches(transaction)).isTrue();
    }

    @Test
    void declinesWhenTheTransactionDoesNot() {
        var rule = new DomesticEligibility();
        var transaction = new Transaction(UUID.randomUUID(), 500L, "FR");

        assertThat(rule.matches(transaction)).isFalse();
    }
}
