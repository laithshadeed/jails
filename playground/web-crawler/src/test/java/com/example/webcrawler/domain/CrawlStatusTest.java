package com.example.webcrawler.domain;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import org.junit.jupiter.api.Test;

class CrawlStatusTest {

    @Test
    void parsesItsOwnNames() {
        assertThat(CrawlStatus.valueOf("QUEUED")).isEqualTo(CrawlStatus.QUEUED);
    }

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {
        assertThatIllegalArgumentException().isThrownBy(() -> CrawlStatus.valueOf("NOPE"));
    }

    @Test
    void declaresEveryConstantExactlyOnce() {
        assertThat(CrawlStatus.values()).hasSize(5).doesNotHaveDuplicates();
    }
}
