package com.example.demo.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * LargeRewardRule is a pure function of its transaction, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class LargeRewardRuleTest {

    @Disabled("write LargeRewardRule first: this names what to prove, it does not prove it")
    @Test
    void grantsWhenTheTransactionQualifies() {
        var largeRewardRule = new LargeRewardRule();
        // TODO: build a qualifying transaction and assert what LargeRewardRule answers.
    }

    @Disabled("write LargeRewardRule first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheTransactionDoesNot() {
        var largeRewardRule = new LargeRewardRule();
        // TODO: assert LargeRewardRule declines a transaction it should not match.
    }
}
