package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;

import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class ExactReferenceMatchRuleTest {

    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var left = new Entry("REF-42", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.of("invoice"));
        var right = new Entry("REF-42", LocalDate.of(2026, 8, 2), new Money(900L, "EUR"), Optional.of("different"));

        assertThat(new ExactReferenceMatchRule().apply(new MatchCandidate(left, right)))
                .contains(MatchOutcome.MATCHED);
    }

    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var left = new Entry("REF-42", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.empty());
        var right = new Entry("REF-43", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.empty());

        assertThat(new ExactReferenceMatchRule().apply(new MatchCandidate(left, right)))
                .isEmpty();
    }
}
