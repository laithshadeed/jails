package com.example.demo.jobs;

import com.example.demo.adapters.Json;
import com.example.demo.messaging.MessageReceivedEvent;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** PostgreSQL transactional outbox with leases, bounded retry and stable event identity. */
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

    @Transactional
    public Optional<Claimed> claim() {
        return db.sql("""
                        with candidate as (
                            select id from receive_message_outbox
                            where (state = 'PENDING' and next_attempt_at <= now())
                               or (state = 'RUNNING' and lease_until <= now())
                            order by next_attempt_at, created_at
                            for update skip locked limit 1
                        )
                        update receive_message_outbox events
                        set state = 'RUNNING', attempts = events.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate where events.id = candidate.id
                        returning events.id, events.payload::text as payload, events.attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query((rows, rowNumber) -> new Claimed(
                        rows.getObject("id", UUID.class),
                        Json.parse(rows.getString("payload"), MessageReceivedEvent.class),
                        rows.getInt("attempts")))
                .optional();
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
                                    secs => least(300, cast(power(2, attempts) as integer))),
                            lease_until = null, last_error = :error,
                            completed_at = case when attempts >= max_attempts then now() else null end
                        where id = :id and state = 'RUNNING'
                        """).param("id", id).param("error", error).update();
    }

    public enum State { PENDING, RUNNING, SUCCEEDED, FAILED }
    public record Status(UUID id, State state, int attempts,
                         Optional<String> lastError, Optional<Instant> completedAt) {}
    public record Claimed(UUID id, MessageReceivedEvent event, int attempt) {}
}
