package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Entry;
import com.example.demo.domain.MatchCandidate;
import com.example.demo.domain.MatchOutcome;
import com.example.demo.domain.Money;
import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class FuzzyMemoMatchRuleTest {

    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var date = LocalDate.of(2026, 8, 1);
        var money = new Money(1_000L, "GBP");
        var left = new Entry("BANK-1", date, money, Optional.of(" Coffee,   Lunch "));
        var right = new Entry("BOOK-9", date, money, Optional.of("coffee, lunch"));

        assertThat(new FuzzyMemoMatchRule().apply(new MatchCandidate(left, right)))
                .contains(MatchOutcome.MATCHED);
    }

    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var date = LocalDate.of(2026, 8, 1);
        var money = new Money(1_000L, "GBP");
        var left = new Entry("BANK-1", date, money, Optional.of("coffee"));
        var absent = new Entry("BOOK-9", date, money, Optional.empty());
        var different = new Entry("BOOK-9", date, money, Optional.of("tea"));
        var rule = new FuzzyMemoMatchRule();

        assertThat(rule.apply(new MatchCandidate(left, absent))).isEmpty();
        assertThat(rule.apply(new MatchCandidate(left, different))).isEmpty();
    }
}
