package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class PayoutTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Payout(null, 1L, PayoutStatus.values()[0], 1L, Instant.parse("2024-01-01T00:00:00Z")))
                .withMessageContaining("id");
    }
}
