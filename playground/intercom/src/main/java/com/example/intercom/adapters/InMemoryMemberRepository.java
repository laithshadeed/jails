package com.example.intercom.adapters;

import com.example.intercom.app.MemberRepository;
import com.example.intercom.domain.Member;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link MemberRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcMemberRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryMemberRepository implements MemberRepository {

    private final Map<String, Member> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Member> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Member> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Member member) {
        items.put(String.valueOf(member.id()), member);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
