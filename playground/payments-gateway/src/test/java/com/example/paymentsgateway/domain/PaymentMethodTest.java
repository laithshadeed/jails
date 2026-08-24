package com.example.paymentsgateway.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class PaymentMethodTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(PaymentMethod.valueOf("CARD")).isEqualTo(PaymentMethod.CARD);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> PaymentMethod.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(PaymentMethod.values()).hasSize(3).doesNotHaveDuplicates();
    }
}
