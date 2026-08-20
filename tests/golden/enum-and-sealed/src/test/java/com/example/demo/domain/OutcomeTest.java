package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

/**
 * The switch below has no {@code default} on purpose: adding a variant
 * should break this test at compile time, which is the whole reason to seal
 * the type in the first place.
 */
class OutcomeTest {

    private String describe(Outcome result) {
        return switch (result) {
            case Outcome.Accepted v -> "accepted";
            case Outcome.Rejected v -> "rejected";
        };
    }

    @Test
    void describesAccepted() {
        assertThat(describe(new Outcome.Accepted())).isEqualTo("accepted");
    }

    @Test
    void describesRejected() {
        assertThat(describe(new Outcome.Rejected())).isEqualTo("rejected");
    }
}
