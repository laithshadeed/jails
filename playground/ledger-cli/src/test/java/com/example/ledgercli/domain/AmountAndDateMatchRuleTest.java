package com.example.ledgercli.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * AmountAndDateMatchRule is a pure function of its matchCandidate, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class AmountAndDateMatchRuleTest {

    @Disabled("write AmountAndDateMatchRule first: this names what to prove, it does not prove it")
    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var amountAndDateMatchRule = new AmountAndDateMatchRule();
        // TODO: build a qualifying matchCandidate and assert what AmountAndDateMatchRule answers.
    }

    @Disabled("write AmountAndDateMatchRule first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var amountAndDateMatchRule = new AmountAndDateMatchRule();
        // TODO: assert AmountAndDateMatchRule declines a matchCandidate it should not match.
    }
}
