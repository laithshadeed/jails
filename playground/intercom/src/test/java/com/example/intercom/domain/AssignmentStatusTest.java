package com.example.intercom.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class AssignmentStatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(AssignmentStatus.valueOf("ACTIVE")).isEqualTo(AssignmentStatus.ACTIVE);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> AssignmentStatus.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(AssignmentStatus.values()).hasSize(2).doesNotHaveDuplicates();
    }
}
