package com.example.intercom.service;

import com.example.intercom.app.ContactRepository;
import com.example.intercom.domain.Contact;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultCreateContactUseCase implements CreateContactUseCase {

    private final ContactRepository repository;

    public DefaultCreateContactUseCase(ContactRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Contact execute(CreateContactCommand command) {
        Objects.requireNonNull(command, "command is required");
        Contact contact = new Contact(
                command.id(),
                command.workspaceId(),
                command.email(),
                command.displayName(),
                Instant.now(),
                Instant.now());
        repository.save(contact);
        return contact;
    }
}
