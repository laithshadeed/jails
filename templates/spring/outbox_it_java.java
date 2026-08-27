package {{pkg}};

{{command_import}}{{usecase_import}}{{target_import}}{{repo_import}}{{kafka_testcontainers_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

import static org.assertj.core.api.Assertions.assertThat;

{{annotation}}@Import({{KAFKA_TESTCONTAINERS_CONFIG}}.class)
@SpringBootTest(properties = {
        "outbox.{{property}}.initial-delay=PT1H",
        "outbox.{{property}}.max-attempts=2"
})
@org.springframework.transaction.annotation.Transactional
class {{usecase}}OutboxIT {

    @Autowired private {{usecase}}UseCase useCase;
    @Autowired private {{target}}Repository results;
    @Autowired private Jdbc{{usecase}}Outbox outbox;
    @Autowired private {{usecase}}OutboxWorker worker;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;

    @Test
    void businessEffectAndEventAreStagedTogetherThenAllConfiguredSinksCompleteDelivery() {
        var command = new {{usecase}}Command(
                {{args}});

        var result = useCase.execute(command);
        var staged = stagedEventId();

        assertThat(results.findById({{key_argument}}))
                .get().extracting({{target}}::id).isEqualTo(result.id());
        assertThat(outbox.status(staged)).get()
                .extracting(Jdbc{{usecase}}Outbox.Status::state)
                .isEqualTo(Jdbc{{usecase}}Outbox.State.PENDING);

        worker.runOnce();

        assertThat(outbox.status(staged)).get()
                .extracting(Jdbc{{usecase}}Outbox.Status::state)
                .isEqualTo(Jdbc{{usecase}}Outbox.State.SUCCEEDED);
    }

    @Test
    void retriesKeepTheStableEventIdAndTerminalFailureIsInspectable() {
        var command = new {{usecase}}Command(
                {{args}});
        useCase.execute(command);
        var staged = stagedEventId();

        var first = outbox.claim().orElseThrow();
        outbox.fail(first.id(), new IllegalStateException("provider unavailable"));
        db.sql("update {{usecase_snake}}_outbox set next_attempt_at = now() where id = :id")
                .param("id", first.id()).update();
        var second = outbox.claim().orElseThrow();
        outbox.fail(second.id(), new IllegalStateException("provider unavailable"));

        assertThat(second.id()).isEqualTo(staged).isEqualTo(first.id());
        assertThat(outbox.status(staged)).get().satisfies(status -> {
            assertThat(status.state()).isEqualTo(Jdbc{{usecase}}Outbox.State.FAILED);
            assertThat(status.lastError()).contains("provider unavailable");
        });
    }

    @Test
    void aSinkThatAlreadyAcceptedIsNotSentTheEventAgain() {
        var command = new {{usecase}}Command(
                {{args}});
        useCase.execute(command);
        var staged = stagedEventId();

        var first = outbox.claim().orElseThrow();
        assertThat(first.delivered()).isEmpty();
        outbox.delivered(staged, "kafka");
        outbox.delivered(staged, "kafka");
        outbox.fail(first.id(), new IllegalStateException("a later sink failed"));
        db.sql("update {{usecase_snake}}_outbox set next_attempt_at = now() where id = :id")
                .param("id", staged).update();

        // What the relay reads before it decides which sinks to call. Recorded
        // once however often it is reported, and it survives the attempt that
        // failed -- otherwise the sink that succeeded is sent the event again
        // on every retry of the sink that did not.
        assertThat(outbox.claim().orElseThrow().delivered()).containsExactly("kafka");
    }

    /**
     * The staged row's id is the <em>event's</em> id, minted once per event --
     * not the resource's. Reading it back rather than assuming
     * {@code result.id()} is what keeps this test honest about the difference:
     * while the two were wrongly the same value, an outbox that discarded
     * every event after the first about one resource still passed.
     */
    private java.util.UUID stagedEventId() {
        return db.sql("select id from {{usecase_snake}}_outbox")
                .query(java.util.UUID.class).single();
    }
}
