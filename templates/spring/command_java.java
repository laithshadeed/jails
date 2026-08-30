package {{pkg}};

import java.io.PrintStream;

/**
 * The {@code {{word}}} subcommand.
 *
 * <p>{@link #run} returns an exit code instead of calling
 * {@code System.exit}, and takes its output streams as arguments instead of
 * reaching for {@code System.out}. Both exist so a test can drive the whole
 * command in-process and assert on what it printed. Keep {@code main} the
 * only place that exits.
 *
 * <p>jails registered this in the project's dispatcher when it generated the
 * class, so {@code {{word}}} already works. If you need to do it by hand -- a
 * second dispatcher, or one jails could not find -- the line is:
 *
 * <pre>{@code
 * commands.put({{name}}Command.NAME, {{name}}Command::run);
 * }</pre>
 */
public final class {{name}}Command {

    /** The word that selects this command on the command line. */
    public static final String NAME = "{{word}}";

    public static final String USAGE = "usage: {{word}} <argument>";

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private {{name}}Command() {}

    /** Runs the command, returning the exit code the process should end with. */
    public static int run(PrintStream out, PrintStream err, String... args) {
        if (args.length != 1) {
            err.println(USAGE);
            return USAGE_ERROR;
        }

        out.println(args[0]);
        return 0;
    }
}
