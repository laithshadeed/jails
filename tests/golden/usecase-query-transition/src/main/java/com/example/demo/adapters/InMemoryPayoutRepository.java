package com.example.demo.adapters;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import org.springframework.stereotype.Component;

/**
 * {@link PayoutRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>When a real {@code DataSource} arrives, `jails add db` makes
 * {@code JdbcPayoutRepository} the bean and drops the annotation here. Annotating
 * both makes two beans qualify for one injection point, which Spring
 * refuses to choose between.
 */
@Component
public class InMemoryPayoutRepository implements PayoutRepository {

    private final Map<String, Payout> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Payout> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Payout> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Payout payout) {
        items.put(String.valueOf(payout.id()), payout);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
