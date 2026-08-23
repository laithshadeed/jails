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

        assertThat(results.findById(String.valueOf(result.id())))
                .get().extracting({{target}}::id).isEqualTo(result.id());
        assertThat(outbox.status(result.id())).get()
                .extracting(Jdbc{{usecase}}Outbox.Status::state)
                .isEqualTo(Jdbc{{usecase}}Outbox.State.PENDING);

        worker.runOnce();

        assertThat(outbox.status(result.id())).get()
                .extracting(Jdbc{{usecase}}Outbox.Status::state)
                .isEqualTo(Jdbc{{usecase}}Outbox.State.SUCCEEDED);
    }

    @Test
    void retriesKeepTheStableEventIdAndTerminalFailureIsInspectable() {
        var command = new {{usecase}}Command(
                {{args}});
        var result = useCase.execute(command);

        var first = outbox.claim().orElseThrow();
        outbox.fail(first.id(), new IllegalStateException("provider unavailable"));
        db.sql("update {{usecase_snake}}_outbox set next_attempt_at = now() where id = :id")
                .param("id", first.id()).update();
        var second = outbox.claim().orElseThrow();
        outbox.fail(second.id(), new IllegalStateException("provider unavailable"));

        assertThat(second.id()).isEqualTo(result.id()).isEqualTo(first.id());
        assertThat(outbox.status(result.id())).get().satisfies(status -> {
            assertThat(status.state()).isEqualTo(Jdbc{{usecase}}Outbox.State.FAILED);
            assertThat(status.lastError()).contains("provider unavailable");
        });
    }
}
