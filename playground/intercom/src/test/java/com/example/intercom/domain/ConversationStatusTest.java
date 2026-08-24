package com.example.intercom.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class ConversationStatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(ConversationStatus.valueOf("OPEN")).isEqualTo(ConversationStatus.OPEN);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> ConversationStatus.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(ConversationStatus.values()).hasSize(3).doesNotHaveDuplicates();
    }
}
