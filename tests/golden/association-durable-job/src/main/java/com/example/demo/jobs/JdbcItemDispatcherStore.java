package com.example.demo.jobs;

import java.sql.ResultSet;
import java.sql.SQLException;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** PostgreSQL queue with skip-locked claiming, leases, bounded retry and terminal failure. */
@Component
public class JdbcItemDispatcherStore implements ItemDispatcherQueue {

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public JdbcItemDispatcherStore(
            JdbcClient db,
            @Value("${jobs.item-dispatcher.max-attempts:10}") int maxAttempts,
            @Value("${jobs.item-dispatcher.lease-seconds:30}") int leaseSeconds) {
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) {
            throw new IllegalArgumentException("max attempts and lease seconds must be positive");
        }
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }

    @Override
    @Transactional
    public void enqueue(ItemDispatcherWork work) {
        Objects.requireNonNull(work, "work is required");
        int inserted = db.sql("""
                        insert into item_dispatcher_jobs (id, owner_id, name, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values (:id, :owner_id, :name, 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
                .param("id", work.id())
                .param("owner_id", work.ownerId())
                .param("name", work.name())
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0) {
            var existing = findWork(work.id()).orElseThrow();
            if (!existing.equals(work)) {
                throw new ItemDispatcherQueue.IdempotencyConflictException(work.id());
            }
        }
    }

    @Override
    public Optional<Status> status(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select id, state, attempts, next_attempt_at, last_error, completed_at
                        from item_dispatcher_jobs
                        where id = :id
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new Status(
                        rows.getObject("id", UUID.class),
                        State.valueOf(rows.getString("state")),
                        rows.getInt("attempts"),
                        rows.getObject("next_attempt_at", OffsetDateTime.class).toInstant(),
                        Optional.ofNullable(rows.getString("last_error")),
                        Optional.ofNullable(rows.getObject("completed_at", OffsetDateTime.class))
                                .map(OffsetDateTime::toInstant)))
                .optional();
    }

    @Transactional
    public Optional<Claimed> claim() {
        return db.sql("""
                        with candidate as (
                            select id
                            from item_dispatcher_jobs
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked
                            limit 1
                        )
                        update item_dispatcher_jobs jobs
                        set state = 'RUNNING',
                            attempts = jobs.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate
                        where jobs.id = candidate.id
                        returning jobs.id as id, jobs.owner_id as owner_id, jobs.name as name, jobs.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query(JdbcItemDispatcherStore::mapClaim)
                .optional();
    }

    @Transactional
    public void succeed(UUID id) {
        db.sql("""
                        update item_dispatcher_jobs
                        set state = 'SUCCEEDED', completed_at = now(), lease_until = null,
                            last_error = null
                        where id = :id and state = 'RUNNING'
                        """)
                .param("id", id)
                .update();
    }

    @Transactional
    public void fail(UUID id, RuntimeException failure) {
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        db.sql("""
                        update item_dispatcher_jobs
                        set state = case when attempts >= max_attempts then 'FAILED' else 'PENDING' end,
                            next_attempt_at = now() + make_interval(
                                    secs => least(300, cast(power(2, attempts) as integer))),
                            lease_until = null,
                            last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """)
                .param("id", id)
                .param("error", error)
                .update();
    }

    private Optional<ItemDispatcherWork> findWork(UUID id) {
        return db.sql("select id, owner_id, name from item_dispatcher_jobs where id = :id")
                .param("id", id)
                .query((rows, rowNumber) -> mapWork(rows))
                .optional();
    }

    private static Claimed mapClaim(ResultSet rows, int rowNumber) throws SQLException {
        return new Claimed(mapWork(rows), rows.getInt("attempts"));
    }

    private static ItemDispatcherWork mapWork(ResultSet rows) throws SQLException {
        return new ItemDispatcherWork(
                    rows.getObject("id", UUID.class),
                    rows.getObject("owner_id", UUID.class),
                    rows.getString("name"));
    }

    public record Claimed(ItemDispatcherWork work, int attempt) {}
}
