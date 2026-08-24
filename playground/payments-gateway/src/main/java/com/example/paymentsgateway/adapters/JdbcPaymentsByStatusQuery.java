package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import com.example.paymentsgateway.service.PaymentsByStatusQuery;
import com.example.paymentsgateway.service.PaymentsByStatusQueryPort;
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

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcPaymentsByStatusQuery implements PaymentsByStatusQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

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

    public JdbcPaymentsByStatusQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Payment> execute(PaymentsByStatusQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from payments
                        where merchant_id = :merchant_id
                          and status = :status
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("merchant_id", query.merchantId())
                .param("status", query.status().name())
                .param("max_results", MAX_RESULTS)
                .query(JdbcPaymentsByStatusQuery::map)
                .list();
    }

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
