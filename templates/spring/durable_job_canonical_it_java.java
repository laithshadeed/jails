package {{pkg}};

{{input_import}}{{repository_import}}import java.util.UUID;
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

    @Test
    void acceptedWorkRunsOnceAndReportsItsOutcome() {
        var id = UUID.randomUUID();
        store.enqueue(id, sample());

        assertThat(store.status(id).orElseThrow().state())
                .isEqualTo({{name}}Queue.State.PENDING);
        worker.runOnce();
        assertThat(store.status(id).orElseThrow().state())
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
