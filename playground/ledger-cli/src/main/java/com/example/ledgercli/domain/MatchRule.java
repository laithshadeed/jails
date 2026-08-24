package com.example.ledgercli.domain;

import java.util.Optional;

/**
 * One reason a matchCandidate produces a result.
 *
 * <p>An open set: every implementation is a bean, and Spring collects them
 * into a {@code List<MatchRule>}. Implementations are independent and each one
 * sees every input, so more than one may answer.
 *
 * <p>Take the whole set as a constructor parameter rather than naming
 * implementations one by one -- that is what makes adding one a matter of
 * writing the class and nothing else:
 *
 * {@snippet :
 * private final List<MatchRule> matchRules;
 * Evaluator(List<MatchRule> matchRules) { this.matchRules = List.copyOf(matchRules); }
 * }
 *
 * <p>Evaluation should be pure -- no clock beyond one the implementation was
 * built with, no database, no network -- so the same matchCandidate always
 * yields the same answer.
 */
public interface MatchRule {

    /** What this grants, or empty when the matchCandidate does not qualify. */
    Optional<MatchOutcome> apply(MatchCandidate matchCandidate);
}
