package com.example.intercom.adapters;

import com.example.intercom.app.WorkspaceRepository;
import com.example.intercom.domain.Workspace;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link WorkspaceRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcWorkspaceRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryWorkspaceRepository implements WorkspaceRepository {

    private final Map<String, Workspace> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Workspace> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Workspace> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Workspace workspace) {
        items.put(String.valueOf(workspace.id()), workspace);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
