package com.example.demo.adapters;

import com.example.demo.app.OwnerRepository;
import com.example.demo.domain.Owner;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/**
 * {@link OwnerRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcOwnerRepository implements OwnerRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            name,
            created_at
            """;

    private final JdbcClient db;

    public JdbcOwnerRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Owner> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from owners
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcOwnerRepository::map)
                .optional();
    }

    @Override
    public List<Owner> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from owners
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcOwnerRepository::map)
                .list();
    }

    @Override
    public void save(Owner owner) {
        Objects.requireNonNull(owner, "owner is required");
        db.sql("""
                        insert into owners (id, name, created_at)
                        values (:id, :name, :created_at)
                        """)
                .param("id", owner.id())
                .param("name", owner.name())
                .param("created_at", Timestamp.from(owner.createdAt()))
                .update();
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from owners
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Owner from the current row. */
    private static Owner map(ResultSet rows, int rowNumber) throws SQLException {
        return new Owner(
                rows.getObject("id", UUID.class),
                rows.getString("name"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }
}
