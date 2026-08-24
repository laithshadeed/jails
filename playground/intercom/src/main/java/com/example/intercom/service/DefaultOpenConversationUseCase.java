package com.example.intercom.service;

import com.example.intercom.app.ConversationRepository;
import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultOpenConversationUseCase implements OpenConversationUseCase {

    private final ConversationRepository repository;

    public DefaultOpenConversationUseCase(ConversationRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Conversation execute(OpenConversationCommand command) {
        Objects.requireNonNull(command, "command is required");
        Conversation conversation = new Conversation(
                command.id(),
                command.workspaceId(),
                command.contactId(),
                command.inboxId(),
                ConversationStatus.values()[0],
                Instant.now(),
                0L,
                Instant.now(),
                Instant.now());
        repository.save(conversation);
        return conversation;
    }
}
