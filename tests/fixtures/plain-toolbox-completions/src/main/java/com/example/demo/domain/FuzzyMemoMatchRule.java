package com.example.demo.domain;

import java.util.Locale;
import java.util.Optional;

/** Matches present memos after case and whitespace normalization. */
public final class FuzzyMemoMatchRule implements MatchRule {

    @Override
    public Optional<MatchOutcome> apply(MatchCandidate candidate) {
        var left = normalize(candidate.sourceEntry().memo());
        var right = normalize(candidate.targetEntry().memo());
        if (left.isPresent() && left.equals(right)) {
            return Optional.of(MatchOutcome.MATCHED);
        }
        return Optional.empty();
    }

    private static Optional<String> normalize(Optional<String> memo) {
        return memo.map(String::trim)
                .filter(value -> !value.isEmpty())
                .map(value -> value.replaceAll("\\s+", " ").toLowerCase(Locale.ROOT));
    }
}
