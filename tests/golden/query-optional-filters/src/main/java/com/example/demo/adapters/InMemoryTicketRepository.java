package com.example.demo.adapters;

import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * {@link TicketRepository} in memory, so the application runs before it has
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
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcTicketRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryTicketRepository implements TicketRepository {

    private final Map<Long, Ticket> items = new ConcurrentHashMap<>();
    private final AtomicLong next = new AtomicLong();

    @Override
    public Optional<Ticket> findById(Long id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Ticket> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public Ticket save(Ticket ticket) {
        Long assigned = next.incrementAndGet();
        Ticket stored = new Ticket(
                assigned,
                ticket.status(),
                ticket.category());
        items.put(assigned, stored);
        return stored;
    }

    @Override
    public boolean deleteById(Long id) {
        return items.remove(id) != null;
    }
}
