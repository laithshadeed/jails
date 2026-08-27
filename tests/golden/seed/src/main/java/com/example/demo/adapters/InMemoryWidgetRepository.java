package com.example.demo.adapters;

import com.example.demo.app.WidgetRepository;
import com.example.demo.domain.Widget;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link WidgetRepository} in memory, so the application runs before it has
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
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcWidgetRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryWidgetRepository implements WidgetRepository {

    private final Map<UUID, Widget> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Widget> findById(UUID id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Widget> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Widget save(Widget widget) {
        items.put(widget.id(), widget);
        return widget;
    }

    @Override
    public boolean deleteById(UUID id) {
        return items.remove(id) != null;
    }
}
