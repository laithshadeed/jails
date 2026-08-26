package com.example.demo.domain;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.UUID;
import org.junit.jupiter.api.Test;

/**
 * The test is the point.
 *
 * <p>A generator that quietly produced version 4 again would look identical
 * at every call site and cost b-tree locality on every table it keys. Nothing
 * else in the project can observe the difference.
 */
class TimeOrderedUuidTest {

    @Test
    void isAVersionSevenUuid() {
        UUID id = TimeOrderedUuid.next();

        assertThat(id.version()).isEqualTo(7);
        assertThat(id.variant()).isEqualTo(2);
    }

    @Test
    void carriesTheMintingTimeInItsLeadingBits() {
        long before = System.currentTimeMillis();
        UUID id = TimeOrderedUuid.next();
        long after = System.currentTimeMillis();

        // The top 48 bits are Unix milliseconds. This is what makes the value
        // sortable, and it is the half a random generator would not have.
        assertThat(id.getMostSignificantBits() >>> 16).isBetween(before, after);
    }

    @Test
    void twoCallsAreNeverEqual() {
        assertThat(TimeOrderedUuid.next()).isNotEqualTo(TimeOrderedUuid.next());
    }
}
