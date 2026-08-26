package com.example.demo.service;

import com.example.demo.domain.MatchCandidate;
import com.example.demo.domain.MatchOutcome;
import com.example.demo.domain.MatchRule;
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
