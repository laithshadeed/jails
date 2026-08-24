package com.example.intercom.adapters;

import com.example.intercom.app.InboxMemberRepository;
import com.example.intercom.domain.InboxMember;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link InboxMemberRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcInboxMemberRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryInboxMemberRepository implements InboxMemberRepository {

    private final Map<String, InboxMember> items = new ConcurrentHashMap<>();

    @Override
    public Optional<InboxMember> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<InboxMember> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(InboxMember inboxMember) {
        items.put(String.valueOf(inboxMember.id()), inboxMember);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
