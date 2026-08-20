package {{pkg}};

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;

import static org.assertj.core.api.Assertions.assertThat;

class {{name}}CommandTest {

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    private int run(String... args) {
        return {{name}}Command.run(new PrintStream(out), new PrintStream(err), args);
    }

    @Test
    void succeedsAndPrintsItsArgument() {
        assertThat(run("hello")).isZero();
        assertThat(out.toString()).contains("hello");
        assertThat(err.toString()).isEmpty();
    }

    @Test
    void reportsUsageOnStderrWhenCalledWithoutArguments() {
        assertThat(run()).isEqualTo({{name}}Command.USAGE_ERROR);
        assertThat(err.toString()).contains({{name}}Command.USAGE);
        assertThat(out.toString()).isEmpty();
    }

    @Test
    void rejectsTooManyArguments() {
        assertThat(run("one", "two")).isEqualTo({{name}}Command.USAGE_ERROR);
    }
}
