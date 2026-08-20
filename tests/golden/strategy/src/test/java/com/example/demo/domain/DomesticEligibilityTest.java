package com.example.demo.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * DomesticEligibility is a pure function of its transaction, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class DomesticEligibilityTest {

    @Disabled("write DomesticEligibility first: this names what to prove, it does not prove it")
    @Test
    void matchesWhenTheTransactionQualifies() {
        var domesticEligibility = new DomesticEligibility();
        // TODO: build a qualifying transaction and assert what DomesticEligibility answers.
    }

    @Disabled("write DomesticEligibility first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheTransactionDoesNot() {
        var domesticEligibility = new DomesticEligibility();
        // TODO: assert DomesticEligibility declines a transaction it should not match.
    }
}
