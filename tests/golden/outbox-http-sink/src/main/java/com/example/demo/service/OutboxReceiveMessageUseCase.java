package com.example.demo.service;

import com.example.demo.domain.Message;
import com.example.demo.jobs.JdbcReceiveMessageOutbox;
import com.example.demo.messaging.MessageReceivedEvent;
import java.time.Instant;
import java.util.Objects;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** Creates the resource and stages its event in the same database transaction. */
@Primary
@Component
public class OutboxReceiveMessageUseCase implements ReceiveMessageUseCase {

    private final StoringReceiveMessageUseCase delegate;
    private final JdbcReceiveMessageOutbox outbox;

    public OutboxReceiveMessageUseCase(StoringReceiveMessageUseCase delegate, JdbcReceiveMessageOutbox outbox) {
        this.delegate = Objects.requireNonNull(delegate, "delegate is required");
        this.outbox = Objects.requireNonNull(outbox, "outbox is required");
    }

    @Override
    @Transactional
    public Message execute(ReceiveMessageCommand command) {
        var result = delegate.execute(command);
        outbox.stage(new MessageReceivedEvent(
                result.id(),
                Instant.now()));
        return result;
    }
}
