package com.example.demo.domain;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * CoffeeRewardRule is a pure function of its transaction, so it needs no context,
 * no container and no mocks -- construct it and call it.
 */
class CoffeeRewardRuleTest {

    @Disabled("write CoffeeRewardRule first: this names what to prove, it does not prove it")
    @Test
    void grantsWhenTheTransactionQualifies() {
        var coffeeRewardRule = new CoffeeRewardRule();
        // TODO: build a qualifying transaction and assert what CoffeeRewardRule answers.
    }

    @Disabled("write CoffeeRewardRule first: this names what to prove, it does not prove it")
    @Test
    void declinesWhenTheTransactionDoesNot() {
        var coffeeRewardRule = new CoffeeRewardRule();
        // TODO: assert CoffeeRewardRule declines a transaction it should not match.
    }
}
