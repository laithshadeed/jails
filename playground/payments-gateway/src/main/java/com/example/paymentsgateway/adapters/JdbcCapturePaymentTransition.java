package com.example.paymentsgateway.adapters;

import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import com.example.paymentsgateway.service.CapturePaymentCommand;
import com.example.paymentsgateway.service.CapturePaymentUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap: scoped matches cannot mutate another tenant's row. */
@Component
public class JdbcCapturePaymentTransition implements CapturePaymentUseCase {

    private final JdbcClient db;

    public JdbcCapturePaymentTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public Payment execute(CapturePaymentCommand command) {
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update payments
                        set status = :status,
                            updated_at = current_timestamp,
                            version = version + 1
                        where id = :id
                          and merchant_id = :merchant_id
                          and version = :version
                        returning id, merchant_id, idempotency_key, amount_minor, currency, method, status, version, authorised_at, captured_at, created_at, updated_at
                        """)
                .param("id", command.id())
                .param("merchant_id", command.merchantId())
                .param("status", command.status().name())
                .param("version", command.version())
                .query(JdbcCapturePaymentTransition::map)
                .optional();
        if (updated.isPresent()) return updated.orElseThrow();

        boolean existsInScope = db.sql("""
                        select exists(
                            select 1 from payments
                            where id = :id
                                  and merchant_id = :merchant_id
                        )
                        """)
                .param("id", command.id())
                .param("merchant_id", command.merchantId())
                .query(Boolean.class)
                .single();
        if (existsInScope) throw new CapturePaymentUseCase.StaleVersionException();
        throw new CapturePaymentUseCase.NotFoundException();
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
