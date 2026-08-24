package com.example.ledgercli.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * FuzzyMemoMatchRule is a pure function of its matchCandidate, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class FuzzyMemoMatchRuleTest {

    @Disabled("write FuzzyMemoMatchRule first: this names what to prove, it does not prove it")
    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var fuzzyMemoMatchRule = new FuzzyMemoMatchRule();
        // TODO: build a qualifying matchCandidate and assert what FuzzyMemoMatchRule answers.
    }

    @Disabled("write FuzzyMemoMatchRule first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var fuzzyMemoMatchRule = new FuzzyMemoMatchRule();
        // TODO: assert FuzzyMemoMatchRule declines a matchCandidate it should not match.
    }
}
