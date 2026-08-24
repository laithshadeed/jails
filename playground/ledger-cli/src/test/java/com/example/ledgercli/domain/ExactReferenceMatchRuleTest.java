package com.example.ledgercli.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * ExactReferenceMatchRule is a pure function of its matchCandidate, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class ExactReferenceMatchRuleTest {

    @Disabled("write ExactReferenceMatchRule first: this names what to prove, it does not prove it")
    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var exactReferenceMatchRule = new ExactReferenceMatchRule();
        // TODO: build a qualifying matchCandidate and assert what ExactReferenceMatchRule answers.
    }

    @Disabled("write ExactReferenceMatchRule first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var exactReferenceMatchRule = new ExactReferenceMatchRule();
        // TODO: assert ExactReferenceMatchRule declines a matchCandidate it should not match.
    }
}
