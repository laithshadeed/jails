package com.example.demo.adapters;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * {@link PayoutRepository} over plain JDBC. No ORM: the queries are visible,
 * and a {@code PreparedStatement} is the whole abstraction.
 *
 * <p>The caller owns the {@link Connection} -- this class neither opens nor
 * closes it, so one transaction can span several repositories.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
public final class JdbcPayoutRepository implements PayoutRepository {

    private static final String FIND_BY_ID =
            """
            select
                id,
                amount,
                status,
                version,
                created_at
            from payouts
            where id = ?
            """;
    private static final String FIND_ALL =
            """
            select
                id,
                amount,
                status,
                version,
                created_at
            from payouts
            order by id
            """;
    private static final String INSERT =
            """
            insert into payouts (id, amount, status, version, created_at)
            values (?, ?, ?, ?, ?)
            """;
    private static final String DELETE_BY_ID =
            """
            delete from payouts
            where id = ?
            """;

    private final Connection connection;

    public JdbcPayoutRepository(Connection connection) {
        this.connection = Objects.requireNonNull(connection, "connection is required");
    }

    @Override
    public Optional<Payout> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {
            query.setObject(1, id);
            try (var rows = query.executeQuery()) {
                return rows.next() ? Optional.of(map(rows)) : Optional.empty();
            }
        } catch (SQLException error) {
            throw new IllegalStateException("could not read payouts " + id, error);
        }
    }

    @Override
    public List<Payout> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        try (var query = connection.prepareStatement(FIND_ALL);
                var rows = query.executeQuery()) {
            var all = new ArrayList<Payout>();
            while (rows.next()) {
                all.add(map(rows));
            }
            return List.copyOf(all);
        } catch (SQLException error) {
            throw new IllegalStateException("could not read payouts", error);
        }
    }

    @Override
    public void save(Payout payout) {
        Objects.requireNonNull(payout, "payout is required");
        try (var insert = connection.prepareStatement(INSERT)) {
            bind(insert, payout);
            insert.executeUpdate();
        } catch (SQLException error) {
            throw new IllegalStateException("could not save to payouts", error);
        }
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        try (var delete = connection.prepareStatement(DELETE_BY_ID)) {
            delete.setObject(1, id);
            return delete.executeUpdate() > 0;
        } catch (SQLException error) {
            throw new IllegalStateException("could not delete from payouts " + id, error);
        }
    }

    /** Builds a Payout from the current row. */
    private Payout map(ResultSet rows) throws SQLException {
        return new Payout(
                rows.getObject("id", UUID.class),
                rows.getLong("amount"),
                PayoutStatus.valueOf(rows.getString("status")),
                rows.getLong("version"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }

    /** Sets every column the insert above declares, in that order. */
    private void bind(java.sql.PreparedStatement insert, Payout payout) throws SQLException {
        insert.setObject(1, payout.id());
        insert.setObject(2, payout.amount());
        insert.setObject(3, payout.status().name());
        insert.setObject(4, payout.version());
        insert.setObject(5, Timestamp.from(payout.createdAt()));
    }
}
