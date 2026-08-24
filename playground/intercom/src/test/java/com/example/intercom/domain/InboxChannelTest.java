package com.example.intercom.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class InboxChannelTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(InboxChannel.valueOf("WEB")).isEqualTo(InboxChannel.WEB);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> InboxChannel.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(InboxChannel.values()).hasSize(2).doesNotHaveDuplicates();
    }
}
