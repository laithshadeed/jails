package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.app.RefundRepository;
import com.example.paymentsgateway.domain.Refund;
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
 * {@link RefundRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcRefundRepository implements RefundRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            merchant_id,
            payment_id,
            amount_minor,
            reason,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcRefundRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Refund> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from refunds
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcRefundRepository::map)
                .optional();
    }

    @Override
    public List<Refund> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from refunds
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcRefundRepository::map)
                .list();
    }

    @Override
    public void save(Refund refund) {
        Objects.requireNonNull(refund, "refund is required");
        db.sql("""
                        insert into refunds (id, merchant_id, payment_id, amount_minor, reason, created_at, updated_at)
                        values (:id, :merchant_id, :payment_id, :amount_minor, :reason, :created_at, :updated_at)
                        """)
                .param("id", refund.id())
                .param("merchant_id", refund.merchantId())
                .param("payment_id", refund.paymentId())
                .param("amount_minor", refund.amountMinor())
                .param("reason", refund.reason().orElse(null))
                .param("created_at", Timestamp.from(refund.createdAt()))
                .param("updated_at", Timestamp.from(refund.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from refunds
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Refund from the current row. */
    private static Refund map(ResultSet rows, int rowNumber) throws SQLException {
        return new Refund(
                rows.getObject("id", UUID.class),
                rows.getObject("merchant_id", UUID.class),
                rows.getObject("payment_id", UUID.class),
                rows.getLong("amount_minor"),
                Optional.ofNullable(rows.getString("reason")),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
