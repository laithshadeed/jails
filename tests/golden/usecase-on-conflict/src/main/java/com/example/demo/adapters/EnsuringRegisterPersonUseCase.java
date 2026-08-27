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
 * <p>Not the repository, deliberately. A port with a {@code save(T)} cannot
 * express {@code on conflict do nothing returning}, and the obvious
 * alternative -- read, then insert if absent -- leaves a window where two
 * callers both see nothing and both proceed. Whichever one loses the race gets
 * a constraint violation instead of the row it asked for.
 *
 * <p>The insert either wins and returns its row, or does nothing and returns
 * none; the second statement then reads the row that was already there. Both
 * are inside one transaction, so a caller sees a created row or an existing
 * one and never a half of either.
 *
 * <p>The conflict column must carry a unique index, or {@code on conflict}
 * has nothing to arbitrate against. jails cannot check that from here: a
 * record read off disk carries no constraints, so it says what type the
 * component is and not whether the column is unique. The generated {@code IT}
 * checks it instead, against a real database, where the answer is a fact
 * rather than a claim.
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
