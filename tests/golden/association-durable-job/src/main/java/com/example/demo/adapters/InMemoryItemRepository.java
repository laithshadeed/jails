package com.example.demo.adapters;

import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link ItemRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcItemRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryItemRepository implements ItemRepository {

    private final Map<String, Item> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Item> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Item> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Item item) {
        items.put(String.valueOf(item.id()), item);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
