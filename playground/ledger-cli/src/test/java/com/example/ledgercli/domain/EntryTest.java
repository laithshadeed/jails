package com.example.ledgercli.domain;

import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class EntryTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Entry(null, LocalDate.of(2024, 1, 1), new Money(1L, "sample"), Optional.empty()))
                .withMessageContaining("reference");
    }
}
