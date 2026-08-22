package com.example.demo.adapters;

import com.example.demo.app.OwnerRepository;
import com.example.demo.domain.Owner;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link OwnerRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcOwnerRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryOwnerRepository implements OwnerRepository {

    private final Map<String, Owner> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Owner> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Owner> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Owner owner) {
        items.put(String.valueOf(owner.id()), owner);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
