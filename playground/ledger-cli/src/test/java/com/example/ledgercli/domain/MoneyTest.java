package com.example.ledgercli.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;

class MoneyTest {

    @Test
    void keepsWhatItWasGiven() {
        var value = Money.of(1L, "sample");

        assertThat(value.amountMinor()).isEqualTo(1L);
        assertThat(value.currency()).isEqualTo("sample");
    }

    @Test
    void rejectsANullComponent() {
        assertThatThrownBy(() -> Money.of(1L, null))
                .isInstanceOf(NullPointerException.class)
                .hasMessageContaining("currency");
    }

    @Test
    void trimsSurroundingWhitespace() {
        assertThat(Money.of(1L, "  trimmed  ").currency()).isEqualTo("trimmed");
    }

    /** Blank is the failure a null check never catches. */
    @Test
    void rejectsBlankText() {
        assertThatThrownBy(() -> Money.of(1L, "   "))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("currency");
    }
}
