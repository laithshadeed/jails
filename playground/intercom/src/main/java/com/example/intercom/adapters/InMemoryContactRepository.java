package com.example.intercom.adapters;

import com.example.intercom.app.ContactRepository;
import com.example.intercom.domain.Contact;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link ContactRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcContactRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryContactRepository implements ContactRepository {

    private final Map<String, Contact> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Contact> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Contact> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Contact contact) {
        items.put(String.valueOf(contact.id()), contact);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
