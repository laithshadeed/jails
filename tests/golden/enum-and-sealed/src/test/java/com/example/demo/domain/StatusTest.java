package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class StatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(Status.valueOf("ACTIVE")).isEqualTo(Status.ACTIVE);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> Status.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(Status.values()).hasSize(2).doesNotHaveDuplicates();
    }
}
