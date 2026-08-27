package com.example.demo.adapters;

import com.example.demo.domain.Person;
import com.example.demo.domain.TimeOrderedUuid;
import com.example.demo.service.RegisterPersonCommand;
import com.example.demo.service.RegisterPersonUseCase;
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

/**
 * Get-or-create keyed on {@code email}, as one statement.
 *
 * <p>Not the repository: a port with a {@code save(T)} cannot express
 * {@code on conflict do nothing returning}, and read-then-insert leaves the
 * window where two callers both see nothing and both proceed. The insert wins
 * and returns its row, or does nothing and the second statement reads the row
 * that was already there -- one transaction, so never half of either.
 *
 * <p>The conflict column must carry a unique index. jails cannot check that
 * from here, because a record read off disk carries no constraints; the
 * generated {@code IT} checks it against a real database instead.
 */
@Component
public class EnsuringRegisterPersonUseCase implements RegisterPersonUseCase {

    private final JdbcClient db;

    public EnsuringRegisterPersonUseCase(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public Person execute(RegisterPersonCommand command) {
        Objects.requireNonNull(command, "command is required");
        Person candidate = new Person(
                TimeOrderedUuid.next(),
                command.email(),
                Instant.now());
        var created = db.sql("""
                        insert into people (id, email, created_at)
                        values (:id, :email, :created_at)
                        on conflict (lower(email)) do nothing
                        returning id, email, created_at
                        """)
                .param("id", candidate.id())
                .param("email", candidate.email())
                .param("created_at", Timestamp.from(candidate.createdAt()))
                .query(EnsuringRegisterPersonUseCase::map)
                .optional();
        if (created.isPresent()) {
            return created.orElseThrow();
        }

        // Somebody else has this key. Read their row rather than reporting a
        // conflict: that is the whole difference between this and an insert.
        return db.sql("""
                        select id, email, created_at
                        from people
                        where lower(email) = lower(:email)
                        """)
                .param("email", candidate.email())
                .query(EnsuringRegisterPersonUseCase::map)
                .optional()
                .orElseThrow(() -> new IllegalStateException(
                        "Person with this email was claimed by a transaction "
                                + "that then rolled back; retry"));
    }

    private static Person map(ResultSet rows, int rowNumber) throws SQLException {
        return new Person(
                rows.getObject("id", UUID.class),
                rows.getString("email"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }
}
