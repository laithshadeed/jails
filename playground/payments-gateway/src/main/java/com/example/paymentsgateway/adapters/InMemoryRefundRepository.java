package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.app.RefundRepository;
import com.example.paymentsgateway.domain.Refund;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link RefundRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcRefundRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryRefundRepository implements RefundRepository {

    private final Map<String, Refund> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Refund> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Refund> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Refund refund) {
        items.put(String.valueOf(refund.id()), refund);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
