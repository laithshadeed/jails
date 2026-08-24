package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.app.MerchantRepository;
import com.example.paymentsgateway.domain.Merchant;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link MerchantRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcMerchantRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryMerchantRepository implements MerchantRepository {

    private final Map<String, Merchant> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Merchant> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Merchant> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Merchant merchant) {
        items.put(String.valueOf(merchant.id()), merchant);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
