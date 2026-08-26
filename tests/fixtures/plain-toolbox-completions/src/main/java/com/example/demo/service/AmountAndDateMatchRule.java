package com.example.demo.service;

import com.example.demo.domain.MatchCandidate;
import com.example.demo.domain.MatchOutcome;
import com.example.demo.domain.MatchRule;
import java.util.Optional;

/** Matches ledger entries whose money and posting date are both equal. */
public final class AmountAndDateMatchRule implements MatchRule {

    @Override
    public Optional<MatchOutcome> apply(MatchCandidate candidate) {
        if (candidate.sourceEntry().amount().equals(candidate.targetEntry().amount())
                && candidate
                        .sourceEntry()
                        .postedAt()
                        .equals(candidate.targetEntry().postedAt())) {
            return Optional.of(MatchOutcome.MATCHED);
        }
        return Optional.empty();
    }
}
