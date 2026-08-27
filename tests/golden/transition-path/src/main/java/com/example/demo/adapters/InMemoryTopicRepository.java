package com.example.demo.adapters;

import com.example.demo.app.TopicRepository;
import com.example.demo.domain.Topic;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * {@link TopicRepository} in memory, so the application runs before it has
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
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcTopicRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryTopicRepository implements TopicRepository {

    private final Map<Long, Topic> items = new ConcurrentHashMap<>();
    private final AtomicLong next = new AtomicLong();

    @Override
    public Optional<Topic> findById(Long id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Topic> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Topic save(Topic topic) {
        Long assigned = next.incrementAndGet();
        Topic stored = new Topic(
                assigned,
                topic.userId(),
                topic.subject(),
                topic.version());
        items.put(assigned, stored);
        return stored;
    }

    @Override
    public boolean deleteById(Long id) {
        return items.remove(id) != null;
    }
}
