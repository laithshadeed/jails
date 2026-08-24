package com.example.intercom.service;

import com.example.intercom.app.ContactRepository;
import com.example.intercom.domain.Contact;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Contact}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class ContactService {

    private final ContactRepository repository;

    public ContactService(ContactRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Contact> findAll() {
        return repository.findAll();
    }

    public Optional<Contact> findById(String id) {
        return repository.findById(id);
    }

    public Contact create(Contact contact) {
        repository.save(contact);
        return contact;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
