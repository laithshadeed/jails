package com.example.demo.domain;

import java.util.Optional;

/**
 * One reason a transaction produces a result.
 *
 * <p>An open set: every implementation is a bean, and Spring collects them
 * into a {@code List<RewardRule>}. Implementations are independent and each one
 * sees every input, so more than one may answer.
 *
 * <p>Take the whole set as a constructor parameter rather than naming
 * implementations one by one -- that is what makes adding one a matter of
 * writing the class and nothing else:
 *
 * {@snippet :
 * private final List<RewardRule> rewardRules;
 * Evaluator(List<RewardRule> rewardRules) { this.rewardRules = List.copyOf(rewardRules); }
 * }
 *
 * <p>Evaluation should be pure -- no clock beyond one the implementation was
 * built with, no database, no network -- so the same transaction always
 * yields the same answer.
 */
public interface RewardRule {

    /** What this grants, or empty when the transaction does not qualify. */
    Optional<Reward> apply(Transaction transaction);
}
