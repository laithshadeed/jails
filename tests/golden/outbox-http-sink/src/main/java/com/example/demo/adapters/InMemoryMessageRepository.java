package com.example.demo.adapters;

import com.example.demo.app.MessageRepository;
import com.example.demo.domain.Message;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link MessageRepository} in memory, so the application runs before it has
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
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcMessageRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryMessageRepository implements MessageRepository {

    private final Map<UUID, Message> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Message> findById(UUID id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Message> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Message save(Message message) {
        items.put(message.id(), message);
        return message;
    }

    @Override
    public boolean deleteById(UUID id) {
        return items.remove(id) != null;
    }
}
