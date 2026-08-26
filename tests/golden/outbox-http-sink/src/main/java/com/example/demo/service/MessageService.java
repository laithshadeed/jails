package com.example.demo.service;

import com.example.demo.app.MessageRepository;
import com.example.demo.domain.Message;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Message}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class MessageService {

    private final MessageRepository repository;

    public MessageService(MessageRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Message> findAll() {
        return repository.findAll();
    }

    public Optional<Message> findById(UUID id) {
        return repository.findById(id);
    }

    public Message create(Message message) {
        return repository.save(message);
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
