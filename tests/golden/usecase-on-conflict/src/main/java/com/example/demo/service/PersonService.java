package com.example.demo.service;

import com.example.demo.app.PersonRepository;
import com.example.demo.domain.Person;
import com.example.demo.domain.TimeOrderedUuid;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Person}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class PersonService {

    private final PersonRepository repository;

    public PersonService(PersonRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Person> findAll() {
        return repository.findAll();
    }

    public Optional<Person> findById(UUID id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Person create(Person person) {
        return repository.save(new Person(
                TimeOrderedUuid.next(),
                person.email(),
                person.createdAt()));
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
