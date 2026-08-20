package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;

class MoneyTest {

    @Test
    void keepsWhatItWasGiven() {
        var value = Money.of(1L, "sample");

        assertThat(value.amount()).isEqualTo(1L);
        assertThat(value.currency()).isEqualTo("sample");
    }

    @Test
    void rejectsANullComponent() {
        assertThatThrownBy(() -> Money.of(1L, null))
                .isInstanceOf(NullPointerException.class)
                .hasMessageContaining("currency");
    }
}
