package com.example.demo.domain;

import java.util.Optional;

/** Matches ledger entries that carry the same external reference. */
public final class ExactReferenceMatchRule implements MatchRule {

    @Override
    public Optional<MatchOutcome> apply(MatchCandidate candidate) {
        if (candidate.sourceEntry().reference().equals(candidate.targetEntry().reference())) {
            return Optional.of(MatchOutcome.MATCHED);
        }
        return Optional.empty();
    }
}
