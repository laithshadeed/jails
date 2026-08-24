package com.example.intercom.service;

import com.example.intercom.domain.Message;
import com.example.intercom.jobs.JdbcReceiveMessageOutbox;
import com.example.intercom.messaging.MessageReceivedEvent;
import java.time.Instant;
import java.util.Objects;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** Creates the resource and stages its event in the same database transaction. */
@Primary
@Component
public class OutboxReceiveMessageUseCase implements ReceiveMessageUseCase {

    private final DefaultReceiveMessageUseCase delegate;
    private final JdbcReceiveMessageOutbox outbox;

    public OutboxReceiveMessageUseCase(DefaultReceiveMessageUseCase delegate, JdbcReceiveMessageOutbox outbox) {
        this.delegate = Objects.requireNonNull(delegate, "delegate is required");
        this.outbox = Objects.requireNonNull(outbox, "outbox is required");
    }

    @Override
    @Transactional
    public Message execute(ReceiveMessageCommand command) {
        var result = delegate.execute(command);
        outbox.stage(new MessageReceivedEvent(
                result.id(),
                result.workspaceId(),
                result.conversationId(),
                Instant.now()));
        return result;
    }
}
