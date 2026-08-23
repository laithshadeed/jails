package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThatCode;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class TallyTest {

    @Test
    void requiresNonnegativeComponents() {
        assertThatIllegalArgumentException().isThrownBy(() -> new Tally(-1, 0L)).withMessageContaining("hits");
        assertThatIllegalArgumentException().isThrownBy(() -> new Tally(0, -1L)).withMessageContaining("total");
        assertThatCode(() -> new Tally(0, 0L)).doesNotThrowAnyException();
    }
}
