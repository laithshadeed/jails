package com.example.demo.testkit;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * Runs a command in-process and captures what a user would have seen.
 *
 * <p>No {@code System.setOut} anywhere: the command under test takes its
 * streams as arguments, so capturing them is just passing different ones. That
 * keeps these tests safe to run in parallel, which the swap-the-global approach
 * never is.
 *
 * <p>{@link Command} matches the shape {@code jails generate command} and
 * {@code jails generate cli} emit, so a real command is a method reference:
 *
 * {@snippet :
 * var run = Cli.run(GreetCommand::run, "world");
 * assertThat(run.exitCode()).isZero();
 * assertThat(run.out()).contains("hello world");
 * }
 */
public final class Cli {

    /** Anything that takes streams plus argv and returns an exit code. */
    @FunctionalInterface
    public interface Command {
        int run(PrintStream out, PrintStream err, String... args);
    }

    /** What one invocation produced. */
    public record Run(String out, String err, int exitCode) {

        /** Stdout split into non-blank lines, for asserting line by line. */
        public List<String> outLines() {
            return out.lines().filter(line -> !line.isBlank()).toList();
        }

        public boolean succeeded() {
            return exitCode == 0;
        }
    }

    private Cli() {}

    public static Run run(Command command, String... args) {
        var out = new ByteArrayOutputStream();
        var err = new ByteArrayOutputStream();
        int exitCode;
        try (var capturedOut = new PrintStream(out, true, StandardCharsets.UTF_8);
                var capturedErr = new PrintStream(err, true, StandardCharsets.UTF_8)) {
            exitCode = command.run(capturedOut, capturedErr, args);
        }
        return new Run(out.toString(StandardCharsets.UTF_8), err.toString(StandardCharsets.UTF_8), exitCode);
    }
}
