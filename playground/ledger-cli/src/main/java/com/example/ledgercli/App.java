package com.example.ledgercli;

import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

/**
 * Argv dispatch for the ledger-cli command line: it owns argument routing, exit
 * codes and streams, and nothing else.
 *
 * <p>The registry is a parameter of {@link #run}, not a static the method
 * reaches for. That is what lets a test drive the whole dispatcher with its own
 * commands, without a real one existing and without touching
 * {@code System.out}. {@link #commands()} is the one place to edit as you add
 * commands; {@code main} is the only place that exits.
 *
 * {@snippet :
 * var out = new ByteArrayOutputStream();
 * int code = App.run(App.commands(), new PrintStream(out), System.err, "greet", "world");
 * }
 */
public final class App {

    /**
     * One subcommand. Matches the shape {@code jails generate command} emits,
     * so {@code SomethingCommand::run} is a method reference away.
     */
    @FunctionalInterface
    public interface Command {
        int run(PrintStream out, PrintStream err, String... args);
    }

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private App() {}

    /**
     * The commands this CLI answers to, in the order they should be listed.
     *
     * <p>Add yours here -- a {@code SequencedMap} because help output that
     * reorders itself between runs is a diff nobody wants:
     *
     * {@snippet :
     * commands.put(ImportCommand.NAME, ImportCommand::run);
     * }
     */
    public static SequencedMap<String, Command> commands() {
        var commands = new LinkedHashMap<String, Command>();
        return commands;
    }

    /** Runs one invocation and returns the exit code the process should end with. */
    public static int run(SequencedMap<String, Command> commands, PrintStream out, PrintStream err, String... args) {
        var name = args.length == 0 ? "help" : args[0];

        if (name.equals("help") || name.equals("--help") || name.equals("-h")) {
            usage(commands, out);
            return 0;
        }

        var command = commands.get(name);
        if (command == null) {
            err.println("unknown command: " + name);
            usage(commands, err);
            return USAGE_ERROR;
        }

        // Everything after the command word belongs to the command itself.
        var rest = new String[args.length - 1];
        System.arraycopy(args, 1, rest, 0, rest.length);
        return command.run(out, err, rest);
    }

    private static void usage(SequencedMap<String, Command> commands, PrintStream to) {
        to.println("usage: ledger-cli <command> [args]");
        to.println();
        to.println("commands:");
        to.println("  help");
        commands.keySet().forEach(name -> to.println("  " + name));
    }

    public static void main(String[] args) {
        System.exit(run(commands(), System.out, System.err, args));
    }
}
