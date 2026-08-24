package com.example.intercom.service;

import com.example.intercom.app.MessageRepository;
import com.example.intercom.domain.Message;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultReceiveMessageUseCase implements ReceiveMessageUseCase {

    private final MessageRepository repository;

    public DefaultReceiveMessageUseCase(MessageRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Message execute(ReceiveMessageCommand command) {
        Objects.requireNonNull(command, "command is required");
        Message message = new Message(
                command.id(),
                command.workspaceId(),
                command.conversationId(),
                command.direction(),
                command.body(),
                Instant.now(),
                Instant.now());
        repository.save(message);
        return message;
    }
}
