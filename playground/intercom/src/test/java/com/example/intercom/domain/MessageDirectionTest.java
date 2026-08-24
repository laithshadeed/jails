package com.example.intercom.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class MessageDirectionTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(MessageDirection.valueOf("INBOUND")).isEqualTo(MessageDirection.INBOUND);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> MessageDirection.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(MessageDirection.values()).hasSize(2).doesNotHaveDuplicates();
    }
}
