package com.example.intercom.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class MemberRoleTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(MemberRole.valueOf("OWNER")).isEqualTo(MemberRole.OWNER);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> MemberRole.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(MemberRole.values()).hasSize(3).doesNotHaveDuplicates();
    }
}
