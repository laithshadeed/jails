package com.example.demo.domain;

import java.util.Optional;

/**
 * TODO: say what makes a transaction qualify under LargeRewardRule.
 */
public final class LargeRewardRule implements RewardRule {

    @Override
    public Optional<Reward> apply(Transaction transaction) {
        // TODO: decide whether transaction qualifies, and what it earns.
        return Optional.empty();
    }
}
