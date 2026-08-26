package com.example.demo.adapters;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.ChangePayoutStatusCommand;
import com.example.demo.service.ChangePayoutStatusUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap. */
@Component
public class JdbcChangePayoutStatusTransition implements ChangePayoutStatusUseCase {

    private final JdbcClient db;

    public JdbcChangePayoutStatusTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public ChangePayoutStatusUseCase.Result execute(ChangePayoutStatusCommand command, long expectedVersion) {
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update payouts
                        set status = :status,
                            version = version + 1
                        where id = :id
                          and version = :version
                        returning id, amount, status, version, created_at
                        """)
                .param("id", command.id())
                .param("status", command.status().name())
                .param("version", expectedVersion)
                .query(JdbcChangePayoutStatusTransition::map)
                .optional();
        if (updated.isPresent()) {
            return new ChangePayoutStatusUseCase.Result.Applied(updated.orElseThrow());
        }

        // Nothing moved, and the two reasons are different facts: the row is
        // at another version -- in which case the caller wants to see which,
        // and gets it -- or there is no such row at all.
        return db.sql("""
                        select id, amount, status, version, created_at
                        from payouts
                        where id = :id
                        """)
                .param("id", command.id())
                .query(JdbcChangePayoutStatusTransition::map)
                .optional()
                .<ChangePayoutStatusUseCase.Result>map(ChangePayoutStatusUseCase.Result.StaleVersion::new)
                .orElseGet(() -> new ChangePayoutStatusUseCase.Result.NotFound(command.id()));
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
