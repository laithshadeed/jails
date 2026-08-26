package com.example.demo.service;

import com.example.demo.domain.Reward;
import com.example.demo.domain.RewardRule;
import com.example.demo.domain.Transaction;
import java.util.Optional;

/**
 * TODO: say what makes a transaction qualify under CoffeeRewardRule.
 */
public final class CoffeeRewardRule implements RewardRule {

    @Override
    public Optional<Reward> apply(Transaction transaction) {
        // TODO: decide whether transaction qualifies, and what it earns.
        return Optional.empty();
    }
}
