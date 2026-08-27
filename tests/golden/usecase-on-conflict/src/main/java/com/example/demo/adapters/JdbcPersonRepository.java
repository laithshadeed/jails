package com.example.demo.adapters;

import com.example.demo.app.PersonRepository;
import com.example.demo.domain.Person;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/**
 * {@link PersonRepository} over {@link JdbcClient}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {@code ?} list
 * is a silent-swap bug waiting for a schema change -- reorder two columns of
 * the same type and nothing fails to compile and nothing throws.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
@Component
public final class JdbcPersonRepository implements PersonRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            email,
            created_at
            """;

    private final JdbcClient db;

    public JdbcPersonRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Person> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from people
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcPersonRepository::map)
                .optional();
    }

    @Override
    public List<Person> findAll() {
        // Newest first, with the key as the tiebreak so two rows written in
        // the same instant do not swap between two identical requests.
        return db.sql("""
                        select %s
                        from people
                        order by created_at desc, id
                        """.formatted(COLUMNS))
                .query(JdbcPersonRepository::map)
                .list();
    }

    @Override
    public Person save(Person person) {
        Objects.requireNonNull(person, "person is required");
        db.sql("""
                        insert into people (id, email, created_at)
                        values (:id, :email, :created_at)
                        """)
                .param("id", person.id())
                .param("email", person.email())
                .param("created_at", Timestamp.from(person.createdAt()))
                .update();
        return person;
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from people
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Person from the current row. */
    private static Person map(ResultSet rows, int rowNumber) throws SQLException {
        return new Person(
                rows.getObject("id", UUID.class),
                rows.getString("email"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }
}
