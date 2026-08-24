package com.example.ledgercli.cli;

import static org.assertj.core.api.Assertions.assertThat;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import org.junit.jupiter.api.Test;

class ReconcileCommandTest {

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    private int run(String... args) {
        return ReconcileCommand.run(new PrintStream(out), new PrintStream(err), args);
    }

    @Test
    void succeedsAndPrintsItsArgument() {
        assertThat(run("hello")).isZero();
        assertThat(out.toString()).contains("hello");
        assertThat(err.toString()).isEmpty();
    }

    @Test
    void reportsUsageOnStderrWhenCalledWithoutArguments() {
        assertThat(run()).isEqualTo(ReconcileCommand.USAGE_ERROR);
        assertThat(err.toString()).contains(ReconcileCommand.USAGE);
        assertThat(out.toString()).isEmpty();
    }

    @Test
    void rejectsTooManyArguments() {
        assertThat(run("one", "two")).isEqualTo(ReconcileCommand.USAGE_ERROR);
    }
}
