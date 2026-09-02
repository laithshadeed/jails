package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Entry;
import com.example.demo.domain.MatchCandidate;
import com.example.demo.domain.MatchOutcome;
import com.example.demo.domain.Money;
import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class ExactReferenceMatchRuleTest {

    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var left = new Entry("REF-42", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.of("invoice"));
        var right = new Entry("REF-42", LocalDate.of(2026, 8, 2), new Money(900L, "EUR"), Optional.of("different"));

        assertThat(new ExactReferenceMatchRule().evaluate(new MatchCandidate(left, right)))
                .contains(MatchOutcome.MATCHED);
    }

    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var left = new Entry("REF-42", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.empty());
        var right = new Entry("REF-43", LocalDate.of(2026, 8, 1), new Money(1_000L, "GBP"), Optional.empty());

        assertThat(new ExactReferenceMatchRule().evaluate(new MatchCandidate(left, right)))
                .isEmpty();
    }
}
