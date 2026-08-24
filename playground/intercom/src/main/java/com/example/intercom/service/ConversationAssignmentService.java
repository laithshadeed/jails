package com.example.intercom.service;

import com.example.intercom.app.ConversationAssignmentRepository;
import com.example.intercom.domain.ConversationAssignment;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link ConversationAssignment}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class ConversationAssignmentService {

    private final ConversationAssignmentRepository repository;

    public ConversationAssignmentService(ConversationAssignmentRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<ConversationAssignment> findAll() {
        return repository.findAll();
    }

    public Optional<ConversationAssignment> findById(String id) {
        return repository.findById(id);
    }

    public ConversationAssignment create(ConversationAssignment conversationAssignment) {
        repository.save(conversationAssignment);
        return conversationAssignment;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
