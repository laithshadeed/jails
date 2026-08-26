package com.example.demo.service;

import com.example.demo.app.MessageRepository;
import com.example.demo.domain.Message;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code OutboxReceiveMessageUseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class StoringReceiveMessageUseCase implements ReceiveMessageUseCase {

    private final MessageRepository repository;

    public StoringReceiveMessageUseCase(MessageRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Message execute(ReceiveMessageCommand command) {
        Objects.requireNonNull(command, "command is required");
        Message message = new Message(
                command.id(),
                command.body(),
                Instant.now());
        repository.save(message);
        return message;
    }
}
