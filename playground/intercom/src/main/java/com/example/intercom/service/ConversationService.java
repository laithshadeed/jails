package com.example.intercom.service;

import com.example.intercom.app.ConversationRepository;
import com.example.intercom.domain.Conversation;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Conversation}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class ConversationService {

    private final ConversationRepository repository;

    public ConversationService(ConversationRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Conversation> findAll() {
        return repository.findAll();
    }

    public Optional<Conversation> findById(String id) {
        return repository.findById(id);
    }

    public Conversation create(Conversation conversation) {
        repository.save(conversation);
        return conversation;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
