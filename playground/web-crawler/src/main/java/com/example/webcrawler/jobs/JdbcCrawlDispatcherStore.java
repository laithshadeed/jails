package com.example.webcrawler.jobs;

import java.net.URI;
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
public class JdbcCrawlDispatcherStore implements CrawlDispatcherQueue {

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public JdbcCrawlDispatcherStore(
            JdbcClient db,
            @Value("${jobs.crawl-dispatcher.max-attempts:10}") int maxAttempts,
            @Value("${jobs.crawl-dispatcher.lease-seconds:30}") int leaseSeconds) {
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) {
            throw new IllegalArgumentException("max attempts and lease seconds must be positive");
        }
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }

    @Override
    @Transactional
    public void enqueue(CrawlDispatcherWork work) {
        Objects.requireNonNull(work, "work is required");
        int inserted = db.sql("""
                        insert into crawl_dispatcher_jobs (id, seed_url, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values (:id, :seed_url, 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
                .param("id", work.id())
                .param("seed_url", work.seedUrl().toString())
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0) {
            var existing = findWork(work.id()).orElseThrow();
            if (!existing.equals(work)) {
                throw new CrawlDispatcherQueue.IdempotencyConflictException(work.id());
            }
        }
    }

    @Override
    public Optional<Status> status(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select id, state, attempts, next_attempt_at, last_error, completed_at
                        from crawl_dispatcher_jobs
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
                            from crawl_dispatcher_jobs
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked
                            limit 1
                        )
                        update crawl_dispatcher_jobs jobs
                        set state = 'RUNNING',
                            attempts = jobs.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate
                        where jobs.id = candidate.id
                        returning jobs.id as id, jobs.seed_url as seed_url, jobs.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query(JdbcCrawlDispatcherStore::mapClaim)
                .optional();
    }

    @Transactional
    public void succeed(UUID id) {
        db.sql("""
                        update crawl_dispatcher_jobs
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
                        update crawl_dispatcher_jobs
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

    private Optional<CrawlDispatcherWork> findWork(UUID id) {
        return db.sql("select id, seed_url from crawl_dispatcher_jobs where id = :id")
                .param("id", id)
                .query((rows, rowNumber) -> mapWork(rows))
                .optional();
    }

    private static Claimed mapClaim(ResultSet rows, int rowNumber) throws SQLException {
        return new Claimed(mapWork(rows), rows.getInt("attempts"));
    }

    private static CrawlDispatcherWork mapWork(ResultSet rows) throws SQLException {
        return new CrawlDispatcherWork(
                    rows.getObject("id", UUID.class),
                    URI.create(rows.getString("seed_url")));
    }

    public record Claimed(CrawlDispatcherWork work, int attempt) {}
}
