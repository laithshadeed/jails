package com.example.ledgercli.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class MatchOutcomeTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(MatchOutcome.valueOf("MATCHED")).isEqualTo(MatchOutcome.MATCHED);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> MatchOutcome.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(MatchOutcome.values()).hasSize(4).doesNotHaveDuplicates();
    }
}
