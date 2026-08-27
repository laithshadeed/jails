package com.example.demo.adapters;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * {@link NoteRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the {@code id} component, which the database assigns.
 * This fake assigns it too -- from a counter -- because a caller hands in
 * a placeholder and expects the stored value back.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcNoteRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryNoteRepository implements NoteRepository {

    private final Map<Long, Note> items = new ConcurrentHashMap<>();
    private final AtomicLong next = new AtomicLong();

    @Override
    public Optional<Note> findById(Long id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Note> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Note save(Note note) {
        Long assigned = next.incrementAndGet();
        Note stored = new Note(
                assigned,
                note.authorId(),
                note.body(),
                note.senderType());
        items.put(assigned, stored);
        return stored;
    }

    @Override
    public boolean deleteById(Long id) {
        return items.remove(id) != null;
    }
}
