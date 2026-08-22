package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class ItemTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Item(null, UUID.fromString("00000000-0000-0000-0000-000000000001"), "sample", Instant.parse("2024-01-01T00:00:00Z")))
                .withMessageContaining("id");
    }
}
