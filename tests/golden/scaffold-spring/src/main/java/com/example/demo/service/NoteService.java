package com.example.demo.service;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import com.example.demo.domain.TimeOrderedUuid;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Note}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class NoteService {

    private final NoteRepository repository;

    public NoteService(NoteRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Note> findAll() {
        return repository.findAll();
    }

    public Optional<Note> findById(UUID id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Note create(Note note) {
        return repository.save(new Note(
                TimeOrderedUuid.next(),
                note.title(),
                note.createdAt()));
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
