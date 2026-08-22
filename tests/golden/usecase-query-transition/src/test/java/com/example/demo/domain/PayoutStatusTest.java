package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class PayoutStatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(PayoutStatus.valueOf("PENDING")).isEqualTo(PayoutStatus.PENDING);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> PayoutStatus.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(PayoutStatus.values()).hasSize(3).doesNotHaveDuplicates();
    }
}
