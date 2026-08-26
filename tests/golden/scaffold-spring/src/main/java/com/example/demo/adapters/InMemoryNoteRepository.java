package com.example.demo.adapters;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import org.springframework.stereotype.Component;

/**
 * {@link NoteRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the {@code id} component -- the same one the JDBC
 * adapter's {@code where} clause uses.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>When a real {@code DataSource} arrives, `jails add db` makes
 * {@code JdbcNoteRepository} the bean and drops the annotation here. Annotating
 * both makes two beans qualify for one injection point, which Spring
 * refuses to choose between.
 */
@Component
public class InMemoryNoteRepository implements NoteRepository {

    private final Map<UUID, Note> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Note> findById(UUID id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Note> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Note note) {
        items.put(note.id(), note);
    }

    @Override
    public boolean deleteById(UUID id) {
        return items.remove(id) != null;
    }
}
