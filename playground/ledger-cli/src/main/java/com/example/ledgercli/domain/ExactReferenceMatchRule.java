package com.example.ledgercli.domain;

import java.util.Optional;

/**
 * TODO: say what makes a matchCandidate qualify under ExactReferenceMatchRule.
 */
public final class ExactReferenceMatchRule implements MatchRule {

    @Override
    public Optional<MatchOutcome> apply(MatchCandidate matchCandidate) {
        // TODO: decide whether matchCandidate qualifies, and what it earns.
        return Optional.empty();
    }
}
