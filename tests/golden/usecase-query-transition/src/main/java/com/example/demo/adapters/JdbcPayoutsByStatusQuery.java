package com.example.demo.adapters;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.PayoutsByStatusCriteria;
import com.example.demo.service.PayoutsByStatusQuery;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcPayoutsByStatusQuery implements PayoutsByStatusQuery {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            amount,
            status,
            version,
            created_at
            """;

    private final JdbcClient db;

    public JdbcPayoutsByStatusQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Payout> execute(PayoutsByStatusCriteria criteria) {
        Objects.requireNonNull(criteria, "criteria is required");
        return db.sql("""
                        select %s
                        from payouts
                        where status = :status
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("status", criteria.status().name())
                .param("max_results", MAX_RESULTS)
                .query(JdbcPayoutsByStatusQuery::map)
                .list();
    }

    private static Payout map(ResultSet rows, int rowNumber) throws SQLException {
        return new Payout(
                rows.getObject("id", UUID.class),
                rows.getLong("amount"),
                PayoutStatus.valueOf(rows.getString("status")),
                rows.getLong("version"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }
}
