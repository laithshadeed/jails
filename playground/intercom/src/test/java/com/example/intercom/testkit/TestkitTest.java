package com.example.intercom.testkit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.Duration;
import java.time.Instant;
import org.junit.jupiter.api.Test;

/** Proves the test kit itself works, so a failure elsewhere is never its fault. */
class TestkitTest {

    @Test
    void fixedClockDoesNotMove() {
        var clock = Clocks.fixed();

        assertThat(clock.instant()).isEqualTo(Clocks.DEFAULT_START).isEqualTo(clock.instant());
    }

    @Test
    void steppingClockAdvancesOnEveryRead() {
        var clock = Clocks.stepping(Instant.parse("2026-01-01T00:00:00Z"), Duration.ofMinutes(1));

        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:00:00Z"));
        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:01:00Z"));
    }

    @Test
    void idsAreSequentialAndPrefixed() {
        var ids = Ids.sequential("txn");

        assertThat(ids.get()).isEqualTo("txn-1");
        assertThat(ids.get()).isEqualTo("txn-2");
    }

    @Test
    void fixturesLoadOffTheClasspath() {
        assertThat(Fixtures.text("example.json")).contains("bolt");
        assertThat(Fixtures.path("example.json")).exists();
    }

    /** A typo in a fixture name must fail, not quietly read nothing. */
    @Test
    void aMissingFixtureNamesWhatItLookedFor() {
        assertThatThrownBy(() -> Fixtures.text("nope.json"))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("nope.json");
    }

    @Test
    void cliCapturesBothStreamsAndTheExitCode() {
        var run = Cli.run(
                (out, err, args) -> {
                    out.println("out: " + String.join(",", args));
                    err.println("err");
                    return 3;
                },
                "a",
                "b");

        assertThat(run.out()).contains("out: a,b");
        assertThat(run.err()).contains("err");
        assertThat(run.exitCode()).isEqualTo(3);
        assertThat(run.succeeded()).isFalse();
    }
}
