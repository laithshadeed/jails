package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Entry;
import com.example.demo.domain.MatchCandidate;
import com.example.demo.domain.MatchOutcome;
import com.example.demo.domain.Money;
import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class AmountAndDateMatchRuleTest {

    @Test
    void grantsWhenTheMatchCandidateQualifies() {
        var date = LocalDate.of(2026, 8, 1);
        var money = new Money(1_000L, "GBP");
        var left = new Entry("BANK-1", date, money, Optional.of("left"));
        var right = new Entry("BOOK-9", date, money, Optional.of("right"));

        assertThat(new AmountAndDateMatchRule().evaluate(new MatchCandidate(left, right)))
                .contains(MatchOutcome.MATCHED);
    }

    @Test
    void declinesWhenTheMatchCandidateDoesNot() {
        var date = LocalDate.of(2026, 8, 1);
        var money = new Money(1_000L, "GBP");
        var left = new Entry("BANK-1", date, money, Optional.empty());
        var differentDate = new Entry("BOOK-9", date.plusDays(1), money, Optional.empty());
        var differentMoney = new Entry("BOOK-9", date, new Money(1_001L, "GBP"), Optional.empty());
        var rule = new AmountAndDateMatchRule();

        assertThat(rule.evaluate(new MatchCandidate(left, differentDate))).isEmpty();
        assertThat(rule.evaluate(new MatchCandidate(left, differentMoney))).isEmpty();
    }
}
