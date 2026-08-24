package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class VerificationTest {

    @Test
    @Disabled("todo: state what Verification guarantees, then assert it")
    void todo() {
        Verification verification = new Verification(true);

        // Verification has no validation to pin, so assert on what it is
        // *for*. Asserting that an accessor returns what was passed in
        // only tests that javac generated the accessor.
    }
}
