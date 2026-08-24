package com.example.intercom.jobs;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.KafkaTestcontainersConfig;
import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.MessageRepository;
import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import com.example.intercom.service.ReceiveMessageCommand;
import com.example.intercom.service.ReceiveMessageUseCase;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import({KafkaTestcontainersConfig.class, TestcontainersConfig.class})
@SpringBootTest(properties = {
        "outbox.receive-message.initial-delay=PT1H",
        "outbox.receive-message.max-attempts=2"
})
@org.springframework.transaction.annotation.Transactional
class ReceiveMessageOutboxIT {

    @Autowired private ReceiveMessageUseCase useCase;
    @Autowired private MessageRepository results;
    @Autowired private JdbcReceiveMessageOutbox outbox;
    @Autowired private ReceiveMessageOutboxWorker worker;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;

    @Test
    void businessEffectAndEventAreStagedTogetherThenAllConfiguredSinksCompleteDelivery() {
        var command = new ReceiveMessageCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                MessageDirection.values()[0],
                "sample");

        var result = useCase.execute(command);

        assertThat(results.findById(String.valueOf(result.id())))
                .get().extracting(Message::id).isEqualTo(result.id());
        assertThat(outbox.status(result.id())).get()
                .extracting(JdbcReceiveMessageOutbox.Status::state)
                .isEqualTo(JdbcReceiveMessageOutbox.State.PENDING);

        worker.runOnce();

        assertThat(outbox.status(result.id())).get()
                .extracting(JdbcReceiveMessageOutbox.Status::state)
                .isEqualTo(JdbcReceiveMessageOutbox.State.SUCCEEDED);
    }

    @Test
    void retriesKeepTheStableEventIdAndTerminalFailureIsInspectable() {
        var command = new ReceiveMessageCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                MessageDirection.values()[0],
                "sample");
        var result = useCase.execute(command);

        var first = outbox.claim().orElseThrow();
        outbox.fail(first.id(), new IllegalStateException("provider unavailable"));
        db.sql("update receive_message_outbox set next_attempt_at = now() where id = :id")
                .param("id", first.id()).update();
        var second = outbox.claim().orElseThrow();
        outbox.fail(second.id(), new IllegalStateException("provider unavailable"));

        assertThat(second.id()).isEqualTo(result.id()).isEqualTo(first.id());
        assertThat(outbox.status(result.id())).get().satisfies(status -> {
            assertThat(status.state()).isEqualTo(JdbcReceiveMessageOutbox.State.FAILED);
            assertThat(status.lastError()).contains("provider unavailable");
        });
    }
}
