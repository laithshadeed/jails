package {{pkg}};

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

import static org.assertj.core.api.Assertions.assertThat;

class {{class}}Test {

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    /**
     * A registry of test doubles. Because {@code run} takes the commands as an
     * argument, the dispatcher is testable on its own -- these assertions hold
     * before a single real command exists.
     */
    private SequencedMap<String, {{class}}.Command> commands() {
        var commands = new LinkedHashMap<String, {{class}}.Command>();
        commands.put("echo", (out, err, args) -> {
            out.println(String.join(" ", args));
            return 0;
        });
        commands.put("boom", (out, err, args) -> {
            err.println("failed");
            return 1;
        });
        return commands;
    }

    private int run(String... args) {
        return {{class}}.run(commands(), new PrintStream(out), new PrintStream(err), args);
    }

    @Test
    void routesToTheNamedCommandAndPassesTheRestOfArgv() {
        assertThat(run("echo", "hello", "world")).isZero();
        assertThat(out.toString()).contains("hello world");
    }

    @Test
    void returnsWhateverTheCommandReturned() {
        assertThat(run("boom")).isEqualTo(1);
        assertThat(err.toString()).contains("failed");
    }

    @Test
    void listsEveryCommandInHelp() {
        assertThat(run("help")).isZero();
        assertThat(out.toString()).contains("echo").contains("boom");
    }

    @Test
    void treatsNoArgumentsAsHelpRatherThanAnError() {
        assertThat(run()).isZero();
        assertThat(out.toString()).contains("usage:");
    }

    @Test
    void namesTheUnknownCommandAndExitsTwo() {
        assertThat(run("nope")).isEqualTo({{class}}.USAGE_ERROR);
        assertThat(err.toString()).contains("nope");
    }

    /** Help ordering is part of the contract, hence SequencedMap. */
    @Test
    void listsCommandsInRegistrationOrder() {
        run("help");
        var text = out.toString();
        assertThat(text.indexOf("echo")).isLessThan(text.indexOf("boom"));
    }
}
