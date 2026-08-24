package com.example.paymentsgateway.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class PaymentStatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(PaymentStatus.valueOf("AUTHORISED")).isEqualTo(PaymentStatus.AUTHORISED);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> PaymentStatus.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(PaymentStatus.values()).hasSize(6).doesNotHaveDuplicates();
    }
}
