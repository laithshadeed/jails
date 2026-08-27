package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class IssuePriorityTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(IssuePriority.valueOf("NONE")).isEqualTo(IssuePriority.NONE);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> IssuePriority.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(IssuePriority.values()).hasSize(3).doesNotHaveDuplicates();
    }

    /** The name is what the database stores; this is what a client sees. */
    @Test
    void roundTripsEveryWireValue() {
        assertThat(IssuePriority.NONE.wire()).isEqualTo("-");
        assertThat(IssuePriority.HIGH.wire()).isEqualTo("!");
        assertThat(IssuePriority.URGENT.wire()).isEqualTo("!!");
        for (IssuePriority constant : IssuePriority.values()) {
            assertThat(IssuePriority.fromWire(constant.wire())).isEqualTo(constant);
        }
    }

    /** An unknown wire value throws rather than binding to null. */
    @Test
    void rejectsAnUnknownWireValue() {
        assertThatIllegalArgumentException().isThrownBy(() ->          IssuePriority.fromWire("nope"));
    }
}
