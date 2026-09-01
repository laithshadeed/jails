package {{pkg}};

{{input_import}}{{repository_import}}{{sample_imports}}import java.util.UUID;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.jdbc.core.simple.JdbcClient;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/**
 * The two properties a durable queue exists for, against a real database.
 *
 * <p>Neither is observable in a unit test: the first is about what a second
 * process sees, and the second about what survives one dying. The row states
 * are the only evidence either way.
 */
@SpringBootTest
class {{name}}JobIT {

    @Autowired Jdbc{{name}}Store store;
    @Autowired {{name}}Worker worker;
    @Autowired {{target}}Repository results;
    @Autowired JdbcClient db;

    /**
     * Each case starts from an empty queue, because the worker claims the
     * <em>oldest</em> runnable item rather than a named one.
     *
     * <p>Without this a case that enqueues its own work and then drains once
     * drains somebody else's: the row it is asserting about is still PENDING
     * with no attempts, and the failure reads as a queue that does not work.
     * The table is this application's own bookkeeping and the test owns it.
     */
    @BeforeEach
    void emptyTheQueue() {
        db.sql("delete from {{table}}").update();
    }

    /**
     * And the rows the worker committed, because nothing else will.
     *
     * <p>This is the one integration test that must not be
     * {@code @Transactional}: the properties it exists to prove are about what
     * a <em>second</em> process sees, so the worker runs in its own
     * transaction and its writes are real. Every other generated IT rolls back
     * and builds its row from the same sample key -- so leaving this one's
     * output behind hands the next class a primary key that already exists,
     * and which of the two runs first is Failsafe's {@code runOrder}, which is
     * the filesystem's. Green on one machine and a duplicate-key error on the
     * next.
     */
    @AfterEach
    void dropWhatTheWorkerCommitted() {
        db.sql("delete from {{table}}").update();
        db.sql("delete from {{results_table}}").update();
    }

    @Test
    void acceptedWorkRunsOnceAndReportsItsOutcome() {
        var id = UUID.randomUUID();
        store.enqueue(id, sample());

        assertThat(store.status(id).orElseThrow().state())
                .isEqualTo({{name}}Queue.State.PENDING);
        worker.runOnce();
        // The recorded failure, not just the state: "expected SUCCEEDED but
        // was PENDING" is what a retryable failure looks like from outside,
        // and it names neither what failed nor why.
        var after = store.status(id).orElseThrow();
        assertThat(after.state())
                .withFailMessage(
                        "the queued work did not succeed: state=%s attempts=%d error=%s",
                        after.state(), after.attempts(), after.lastError().orElse("none"))
                .isEqualTo({{name}}Queue.State.SUCCEEDED);

        // Draining again must not run it a second time: a SUCCEEDED row is not
        // runnable, which is what stops a restart replaying the whole queue.
        worker.runOnce();
        assertThat(store.status(id).orElseThrow().attempts()).isEqualTo(1);
    }

{{conflict_test}}
    private {{usecase}}Command.Input sample() {
        return new {{usecase}}Command.Input({{args}});
    }
}
