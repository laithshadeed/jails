package com.example.demo.adapters;

import com.example.demo.app.OperatorRepository;
import com.example.demo.domain.Operator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import org.springframework.stereotype.Component;

/**
 * {@link OperatorRepository} in memory, so the application runs before it has
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
 * <p>When a real {@code DataSource} arrives, `jails add db` makes
 * {@code JdbcOperatorRepository} the bean and drops the annotation here. Annotating
 * both makes two beans qualify for one injection point, which Spring
 * refuses to choose between.
 */
@Component
public class InMemoryOperatorRepository implements OperatorRepository {

    private final Map<Long, Operator> items = new ConcurrentHashMap<>();
    private final AtomicLong next = new AtomicLong();

    @Override
    public Optional<Operator> findById(Long id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Operator> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Operator save(Operator operator) {
        Long assigned = next.incrementAndGet();
        Operator stored = new Operator(
                assigned,
                operator.email());
        items.put(assigned, stored);
        return stored;
    }

    @Override
    public boolean deleteById(Long id) {
        return items.remove(id) != null;
    }
}
