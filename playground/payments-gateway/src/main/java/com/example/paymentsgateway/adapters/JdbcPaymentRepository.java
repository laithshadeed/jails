package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
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
 * {@link PaymentRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcPaymentRepository implements PaymentRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            merchant_id,
            idempotency_key,
            amount_minor,
            currency,
            method,
            status,
            version,
            authorised_at,
            captured_at,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcPaymentRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Payment> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from payments
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcPaymentRepository::map)
                .optional();
    }

    @Override
    public List<Payment> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from payments
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcPaymentRepository::map)
                .list();
    }

    @Override
    public void save(Payment payment) {
        Objects.requireNonNull(payment, "payment is required");
        db.sql("""
                        insert into payments (id, merchant_id, idempotency_key, amount_minor, currency, method, status, version, authorised_at, captured_at, created_at, updated_at)
                        values (:id, :merchant_id, :idempotency_key, :amount_minor, :currency, :method, :status, :version, :authorised_at, :captured_at, :created_at, :updated_at)
                        """)
                .param("id", payment.id())
                .param("merchant_id", payment.merchantId())
                .param("idempotency_key", payment.idempotencyKey())
                .param("amount_minor", payment.amountMinor())
                .param("currency", payment.currency())
                .param("method", payment.method().name())
                .param("status", payment.status().name())
                .param("version", payment.version())
                .param("authorised_at", payment.authorisedAt().map(Timestamp::from).orElse(null))
                .param("captured_at", payment.capturedAt().map(Timestamp::from).orElse(null))
                .param("created_at", Timestamp.from(payment.createdAt()))
                .param("updated_at", Timestamp.from(payment.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from payments
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Payment from the current row. */
    private static Payment map(ResultSet rows, int rowNumber) throws SQLException {
        return new Payment(
                rows.getObject("id", UUID.class),
                rows.getObject("merchant_id", UUID.class),
                rows.getString("idempotency_key"),
                rows.getLong("amount_minor"),
                rows.getString("currency"),
                PaymentMethod.valueOf(rows.getString("method")),
                PaymentStatus.valueOf(rows.getString("status")),
                rows.getLong("version"),
                Optional.ofNullable(rows.getObject("authorised_at", OffsetDateTime.class)).map(OffsetDateTime::toInstant),
                Optional.ofNullable(rows.getObject("captured_at", OffsetDateTime.class)).map(OffsetDateTime::toInstant),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
