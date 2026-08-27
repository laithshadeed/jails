package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class SenderTypeTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(SenderType.valueOf("CUSTOMER")).isEqualTo(SenderType.CUSTOMER);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> SenderType.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(SenderType.values()).hasSize(2).doesNotHaveDuplicates();
    }
}
