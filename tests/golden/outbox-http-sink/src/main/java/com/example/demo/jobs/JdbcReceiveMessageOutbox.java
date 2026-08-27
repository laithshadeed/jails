package com.example.demo.jobs;

import com.example.demo.adapters.Json;
import com.example.demo.messaging.MessageReceivedEvent;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * PostgreSQL transactional outbox with leases, bounded retry and stable event
 * identity.
 *
 * <p>The retry interval carries jitter -- {@code 2^attempts} seconds capped at
 * five minutes, then scaled by a random factor between a half and one. Without
 * it every row staged in the same incident retries at the same instant
 * forever, which is how a recovering dependency is knocked back down by the
 * queue that was waiting for it.
 */
@Component
public class JdbcReceiveMessageOutbox {

    private final JdbcClient db;
    private final int maxAttempts;
    private final int leaseSeconds;

    public JdbcReceiveMessageOutbox(
            JdbcClient db,
            @Value("${outbox.receive-message.max-attempts:10}") int maxAttempts,
            @Value("${outbox.receive-message.lease-seconds:30}") int leaseSeconds) {
        this.db = Objects.requireNonNull(db, "db is required");
        if (maxAttempts < 1 || leaseSeconds < 1) throw new IllegalArgumentException("positive limits required");
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
    }

    @Transactional
    public void stage(MessageReceivedEvent event) {
        Objects.requireNonNull(event, "event is required");
        String payload = Json.toJson(event);
        int inserted = db.sql("""
                        insert into receive_message_outbox (id, payload, state, attempts, max_attempts,
                                next_attempt_at, created_at)
                        values (:id, cast(:payload as jsonb), 'PENDING', 0, :maxAttempts, now(), now())
                        on conflict (id) do nothing
                        """)
                .param("id", event.id())
                .param("payload", payload)
                .param("maxAttempts", maxAttempts)
                .update();
        if (inserted == 0) {
            var existing = db.sql("select payload::text from receive_message_outbox where id = :id")
                    .param("id", event.id()).query(String.class).single();
            if (!Json.parse(existing, MessageReceivedEvent.class).equals(event)) {
                throw new IllegalStateException("event id already staged with different payload: " + event.id());
            }
        }
    }

    public Optional<Status> status(UUID id) {
        return db.sql("""
                        select id, state, attempts, last_error, completed_at
                        from receive_message_outbox where id = :id
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new Status(
                        rows.getObject("id", UUID.class),
                        State.valueOf(rows.getString("state")), rows.getInt("attempts"),
                        Optional.ofNullable(rows.getString("last_error")),
                        Optional.ofNullable(rows.getObject("completed_at", OffsetDateTime.class))
                                .map(OffsetDateTime::toInstant)))
                .optional();
    }

    /**
     * Leases up to {@code batchSize} runnable rows in one statement.
     *
     * <p>The batch is the relay's throughput. Claiming one row per scheduler
     * tick caps the whole topic at one event per tick however fast the sinks
     * are, and nothing reports the ceiling -- the queue simply never empties.
     *
     * <p>{@code for update skip locked} is what makes more than one relay
     * safe: a second instance skips the rows this one holds instead of
     * blocking on them.
     */
    @Transactional
    public List<Claimed> claim(int batchSize) {
        if (batchSize < 1) throw new IllegalArgumentException("batchSize must be positive");
        return db.sql("""
                        with candidate as (
                            select id from receive_message_outbox
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked limit :batchSize
                        )
                        update receive_message_outbox events
                        set state = 'RUNNING', attempts = events.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate where events.id = candidate.id
                        returning events.id, events.payload::text as payload, events.attempts,
                            events.delivered
                        """)
                .param("leaseSeconds", leaseSeconds)
                .param("batchSize", batchSize)
                .query((rows, rowNumber) -> new Claimed(
                        rows.getObject("id", UUID.class),
                        Json.parse(rows.getString("payload"), MessageReceivedEvent.class),
                        rows.getInt("attempts"),
                        Set.copyOf(Arrays.asList((String[]) rows.getArray("delivered").getArray()))))
                .list();
    }

    /** One row, for a caller that wants exactly one. */
    @Transactional
    public Optional<Claimed> claim() {
        return claim(1).stream().findFirst();
    }

    /**
     * Records that one sink accepted this event, so a retry does not send it
     * again.
     *
     * <p>A row with several sinks is only as atomic as its worst sink. Without
     * this, a Kafka publish that succeeded followed by an HTTP delivery that
     * failed re-publishes to Kafka on every subsequent attempt -- the
     * consumers see the event once per attempt, and the outbox's
     * at-least-once promise quietly becomes at-least-{@code max_attempts}.
     *
     * <p>Its own transaction on purpose: the point is that it survives the
     * failure of whatever runs next.
     */
    @Transactional
    public void delivered(UUID id, String sink) {
        db.sql("""
                        update receive_message_outbox
                        set delivered = case
                                when cast(:sink as text) = any(delivered) then delivered
                                else array_append(delivered, cast(:sink as text))
                            end
                        where id = :id
                        """).param("id", id).param("sink", sink).update();
    }

    @Transactional
    public void succeed(UUID id) {
        db.sql("""
                        update receive_message_outbox set state = 'SUCCEEDED', lease_until = null,
                            last_error = null, completed_at = now()
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).update();
    }

    @Transactional
    public void fail(UUID id, RuntimeException failure) {
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        db.sql("""
                        update receive_message_outbox
                        set state = case when attempts >= max_attempts then 'FAILED' else 'PENDING' end,
                            next_attempt_at = now() + make_interval(
                                    secs => greatest(1, cast(least(300, power(2, attempts))
                                            * (0.5 + random() / 2) as integer))),
                            lease_until = null, last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).param("error", error).update();
    }

    public enum State { PENDING, RUNNING, SUCCEEDED, FAILED }
    public record Status(UUID id, State state, int attempts,
                         Optional<String> lastError, Optional<Instant> completedAt) {}
    public record Claimed(UUID id, MessageReceivedEvent event, int attempt, Set<String> delivered) {}
}
