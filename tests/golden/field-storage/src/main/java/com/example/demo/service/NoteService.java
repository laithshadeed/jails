package com.example.demo.service;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
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

    public Optional<Note> findById(String id) {
        return repository.findById(id);
    }

    public Note create(Note note) {
        repository.save(note);
        return note;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
