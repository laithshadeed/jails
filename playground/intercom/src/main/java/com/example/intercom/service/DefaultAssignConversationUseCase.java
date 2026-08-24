package com.example.intercom.service;

import com.example.intercom.app.ConversationAssignmentRepository;
import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultAssignConversationUseCase implements AssignConversationUseCase {

    private final ConversationAssignmentRepository repository;

    public DefaultAssignConversationUseCase(ConversationAssignmentRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public ConversationAssignment execute(AssignConversationCommand command) {
        Objects.requireNonNull(command, "command is required");
        ConversationAssignment conversationAssignment = new ConversationAssignment(
                command.id(),
                command.workspaceId(),
                command.conversationId(),
                command.memberId(),
                AssignmentStatus.values()[0],
                0L,
                Instant.now(),
                Instant.now(),
                Instant.now());
        repository.save(conversationAssignment);
        return conversationAssignment;
    }
}
