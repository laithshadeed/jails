package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatNullPointerException;

import org.junit.jupiter.api.Test;

class AuthorTest {

    @Test
    void rejectsANullComponent() {
        assertThatNullPointerException()
                .isThrownBy(() -> new Author(1L, null))
                .withMessageContaining("email");
    }
}
