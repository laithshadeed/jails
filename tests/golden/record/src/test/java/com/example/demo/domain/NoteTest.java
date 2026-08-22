package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import java.time.Instant;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class NoteTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Note(null, Optional.empty(), Instant.parse("2024-01-01T00:00:00Z")))
                .withMessageContaining("title");
    }
}
