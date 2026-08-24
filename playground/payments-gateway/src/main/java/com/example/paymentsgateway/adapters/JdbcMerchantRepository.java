package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.app.MerchantRepository;
import com.example.paymentsgateway.domain.Merchant;
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
 * {@link MerchantRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcMerchantRepository implements MerchantRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            reference,
            display_name,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcMerchantRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Merchant> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from merchants
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcMerchantRepository::map)
                .optional();
    }

    @Override
    public List<Merchant> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from merchants
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcMerchantRepository::map)
                .list();
    }

    @Override
    public void save(Merchant merchant) {
        Objects.requireNonNull(merchant, "merchant is required");
        db.sql("""
                        insert into merchants (id, reference, display_name, created_at, updated_at)
                        values (:id, :reference, :display_name, :created_at, :updated_at)
                        """)
                .param("id", merchant.id())
                .param("reference", merchant.reference())
                .param("display_name", merchant.displayName())
                .param("created_at", Timestamp.from(merchant.createdAt()))
                .param("updated_at", Timestamp.from(merchant.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from merchants
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Merchant from the current row. */
    private static Merchant map(ResultSet rows, int rowNumber) throws SQLException {
        return new Merchant(
                rows.getObject("id", UUID.class),
                rows.getString("reference"),
                rows.getString("display_name"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
