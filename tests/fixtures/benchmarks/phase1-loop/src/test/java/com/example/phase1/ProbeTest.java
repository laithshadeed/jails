package com.example.phase1;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class ProbeTest {
    @Test
    void alpha() {
        assertEquals(4, 2 + 2);
    }

    @Test
    void beta() {
        assertEquals("phase1", "phase" + 1);
    }

    @Test
    void gamma() {
        assertEquals(3, "jdx".length());
    }
}
