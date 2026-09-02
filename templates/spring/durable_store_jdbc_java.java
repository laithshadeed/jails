package {{pkg}};

import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * PostgreSQL work queue: skip-locked claiming, expiring leases, bounded retry
 * and a terminal failure state.
 *
 * <p><strong>The payload is one JSON column, not a column per field.</strong>
 * Nobody queries a work queue by payload field -- it is this application's own
 * bookkeeping rather than the reader's schema -- and a column per field makes
 * the table's shape a function of the command's, so every field added to the
 * command needs a migration for work that has not run yet. The idempotency
 * contract is unaffected: an {@code on conflict (id) do nothing} that inserted
 * nothing means the id is taken, and comparing the decoded payloads answers
 * whether it is the same request.
 *
 * <p>The retry interval carries jitter -- {@code 2^attempts} seconds capped at
 * five minutes, then scaled by a random factor between a half and one. Without
 * it every item enqueued in the same incident retries at the same instant
 * forever, which is how a recovering dependency is knocked back down by the
 * queue that was waiting for it.
 */
@Component
public class Jdbc{{name}}Store implements {{name}}Queue {

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public Jdbc{{name}}Store(
            JdbcClient db,
            @Value("${jobs.{{property}}.max-attempts:10}") int maxAttempts,
            @Value("${jobs.{{property}}.lease-seconds:30}") int leaseSeconds) {
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) {
            throw new IllegalArgumentException("max attempts and lease seconds must be positive");
        }
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }

    @Override
    @Transactional
    public void enqueue(UUID id, {{usecase}}Command.Input work) {
        Objects.requireNonNull(id, "id is required");
        Objects.requireNonNull(work, "work is required");
        int inserted = db.sql("""
                        insert into {{table}} (id, payload, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values (:id, cast(:payload as jsonb), 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
                .param("id", id)
                .param("payload", Json.toJson(work))
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0 && !payloadOf(id).equals(Optional.of(work))) {
            throw new {{name}}Queue.IdempotencyConflictException(id);
        }
    }

    @Override
    public Optional<Status> status(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select id, state, attempts, next_attempt_at, last_error, completed_at
                        from {{table}}
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

    private Optional<{{usecase}}Command.Input> payloadOf(UUID id) {
        return db.sql("select payload::text from {{table}} where id = :id")
                .param("id", id)
                .query(String.class)
                .optional()
                .map(payload -> Json.parse(payload, {{usecase}}Command.Input.class));
    }

    /**
     * Leases one runnable item.
     *
     * <p>{@code for update skip locked} is what makes more than one worker
     * safe: a second instance skips the row this one holds instead of blocking
     * on it. A {@code RUNNING} row whose lease has expired is runnable again,
     * which is how work survives a process that died holding it.
     */
    @Transactional
    public Optional<Claimed> claim() {
        return db.sql("""
                        with candidate as (
                            select id from {{table}}
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked limit 1
                        )
                        update {{table}} jobs
                        set state = 'RUNNING', attempts = jobs.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate where jobs.id = candidate.id
                        returning jobs.id, jobs.payload::text as payload, jobs.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query((rows, rowNumber) -> new Claimed(
                        rows.getObject("id", UUID.class),
                        Json.parse(rows.getString("payload"), {{usecase}}Command.Input.class),
                        rows.getInt("attempts")))
                .optional();
    }

    @Transactional
    public void succeed(UUID id) {
        db.sql("""
                        update {{table}} set state = 'SUCCEEDED', lease_until = null,
                            last_error = null, completed_at = now()
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).update();
    }

    @Transactional
    public void fail(UUID id, RuntimeException failure) {
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        db.sql("""
                        update {{table}}
                        set state = case when attempts >= max_attempts then 'FAILED' else 'PENDING' end,
                            next_attempt_at = now() + make_interval(
                                    secs => greatest(1, cast(least(300, power(2, attempts))
                                            * (0.5 + random() / 2) as integer))),
                            lease_until = null, last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).param("error", error).update();
    }

    public record Claimed(UUID id, {{usecase}}Command.Input work, int attempt) {}
}
