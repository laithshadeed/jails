package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import org.junit.jupiter.api.Test;

class NoteTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Note(1L, null, true, 1L))
                .withMessageContaining("body");
    }
}
