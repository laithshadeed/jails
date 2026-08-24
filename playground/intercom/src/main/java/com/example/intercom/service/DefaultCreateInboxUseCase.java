package com.example.intercom.service;

import com.example.intercom.app.InboxRepository;
import com.example.intercom.domain.Inbox;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultCreateInboxUseCase implements CreateInboxUseCase {

    private final InboxRepository repository;

    public DefaultCreateInboxUseCase(InboxRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Inbox execute(CreateInboxCommand command) {
        Objects.requireNonNull(command, "command is required");
        Inbox inbox = new Inbox(
                command.id(),
                command.workspaceId(),
                command.name(),
                command.channel(),
                Instant.now(),
                Instant.now());
        repository.save(inbox);
        return inbox;
    }
}
