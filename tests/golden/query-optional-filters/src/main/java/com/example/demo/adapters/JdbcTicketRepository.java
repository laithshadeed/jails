package com.example.demo.adapters;

import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.jdbc.support.GeneratedKeyHolder;
import org.springframework.stereotype.Component;

/**
 * {@link TicketRepository} over {@link JdbcClient}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {@code ?} list
 * is a silent-swap bug waiting for a schema change -- reorder two columns of
 * the same type and nothing fails to compile and nothing throws.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
@Component
public final class JdbcTicketRepository implements TicketRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            status,
            category
            """;

    private final JdbcClient db;

    public JdbcTicketRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Ticket> findById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from tickets
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcTicketRepository::map)
                .optional();
    }

    @Override
    public List<Ticket> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by.
        return db.sql("""
                        select %s
                        from tickets
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcTicketRepository::map)
                .list();
    }

    @Override
    public Ticket save(Ticket ticket) {
        Objects.requireNonNull(ticket, "ticket is required");
        var keys = new GeneratedKeyHolder();
        db.sql("""
                        insert into tickets (status, category)
                        values (:status, :category)
                        """)
                .param("status", ticket.status())
                .param("category", ticket.category().orElse(null))
                .update(keys, "id");
        return new Ticket(
                keys.getKeyAs(Long.class),
                ticket.status(),
                ticket.category());
    }

    @Override
    public boolean deleteById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from tickets
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Ticket from the current row. */
    private static Ticket map(ResultSet rows, int rowNumber) throws SQLException {
        return new Ticket(
                rows.getLong("id"),
                rows.getString("status"),
                Optional.ofNullable(rows.getString("category")));
    }
}
