package com.example.demo;

import static org.assertj.core.api.Assertions.assertThatCode;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class MoneyMovedTest {

    @Test
    void requiresAPositiveAmount() {
        assertThatIllegalArgumentException().isThrownBy(() -> new MoneyMoved(0)).withMessageContaining("positive");
        assertThatIllegalArgumentException().isThrownBy(() -> new MoneyMoved(-1));
        assertThatCode(() -> new MoneyMoved(1)).doesNotThrowAnyException();
    }
}
