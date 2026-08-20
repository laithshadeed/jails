package {{pkg}};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThatCode;

/**
 * Calls the work directly rather than waiting for a schedule.
 *
 * <p>A test that sleeps until the scheduler fires is slow and flaky, and it
 * tests Spring's scheduler rather than this job. What is worth asserting here
 * is that {@code run()} does not propagate -- because an exception escaping a
 * scheduled method cancels every future run.
 */
class {{name}}JobTest {

    private final {{name}}Job job = new {{name}}Job();

    @Test
    void theWorkRuns() {
        assertThatCode(job::work).doesNotThrowAnyException();
    }

    @Test
    void aFailureNeverEscapesAndCancelsTheSchedule() {
        {{name}}Job failing =
                new {{name}}Job() {
                    @Override
                    void work() {
                        throw new IllegalStateException("boom");
                    }
                };
        assertThatCode(failing::run).doesNotThrowAnyException();
    }
}
