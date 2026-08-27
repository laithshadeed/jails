package com.example.demo.service;

import com.example.demo.domain.Reward;
import com.example.demo.domain.RewardRule;
import com.example.demo.domain.Transaction;
import java.util.List;
import java.util.Optional;

/**
 * Every RewardRule, asked about the same transaction in one place.
 *
 * <p>The whole set arrives as one constructor parameter, in the caller's
 * order -- which decides the answer: a rule that responds to everything has
 * to come last, or nothing after it is ever reached.
 */
public final class RewardRuleEvaluator {

    private final List<RewardRule> rewardRules;

    public RewardRuleEvaluator(List<RewardRule> rewardRules) {
        this.rewardRules = List.copyOf(rewardRules);
    }

    /** What the first RewardRule to answer grants, or empty when none does. */
    public Optional<Reward> first(Transaction transaction) {
        return rewardRules.stream()
                .map(rule -> rule.apply(transaction))
                .flatMap(Optional::stream)
                .findFirst();
    }

    /** What every RewardRule that answers grants, in order. */
    public List<Reward> all(Transaction transaction) {
        return rewardRules.stream()
                .map(rule -> rule.apply(transaction))
                .flatMap(Optional::stream)
                .toList();
    }
}
