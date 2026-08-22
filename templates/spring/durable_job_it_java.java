package {{pkg}};

{{repository_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

{{annotation}}@SpringBootTest(properties = {
        "jobs.{{property}}.initial-delay=PT1H",
        "jobs.{{property}}.max-attempts=2"
})
@org.springframework.transaction.annotation.Transactional
class {{name}}JobIT {

    @Autowired private {{name}}Queue queue;
    @Autowired private {{name}}Worker worker;
    @Autowired private Jdbc{{name}}Store store;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;
    @Autowired private {{target}}Repository results;

    @Test
    void committedWorkRunsAndRepeatingTheSameIdIsIdempotent() {
        var work = new {{name}}Work(
                {{args}});

        queue.enqueue(work);
        queue.enqueue(work);
        worker.runOnce();

        assertThat(results.findById(String.valueOf(work.id()))).isPresent();
        assertThat(queue.status(work.id())).get()
                .extracting({{name}}Queue.Status::state)
                .isEqualTo({{name}}Queue.State.SUCCEEDED);
    }

    @Test
    void anExpiredLeaseIsReclaimedAndBoundedFailureBecomesVisible() {
        var work = new {{name}}Work(
                {{args}});
        queue.enqueue(work);

        assertThat(store.claim()).isPresent();
        db.sql("update {{table}} set lease_until = now() - interval '1 second' where id = :id")
                .param("id", work.id())
                .update();
        var reclaimed = store.claim().orElseThrow();
        store.fail(work.id(), new IllegalStateException("test failure"));

        assertThat(reclaimed.attempt()).isEqualTo(2);
        assertThat(queue.status(work.id())).get()
                .satisfies(status -> {
                    assertThat(status.state()).isEqualTo({{name}}Queue.State.FAILED);
                    assertThat(status.lastError()).contains("test failure");
                });
    }
{{conflict_test}}
}
