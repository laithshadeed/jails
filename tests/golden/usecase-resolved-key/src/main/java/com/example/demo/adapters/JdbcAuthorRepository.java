package com.example.demo.adapters;

import com.example.demo.app.AuthorRepository;
import com.example.demo.domain.Author;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.jdbc.support.GeneratedKeyHolder;
import org.springframework.stereotype.Component;

/**
 * {@link AuthorRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcAuthorRepository implements AuthorRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            email
            """;

    private final JdbcClient db;

    public JdbcAuthorRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Author> findById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from authors
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcAuthorRepository::map)
                .optional();
    }

    @Override
    public List<Author> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by.
        return db.sql("""
                        select %s
                        from authors
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcAuthorRepository::map)
                .list();
    }

    @Override
    public Author save(Author author) {
        Objects.requireNonNull(author, "author is required");
        var keys = new GeneratedKeyHolder();
        db.sql("""
                        insert into authors (email)
                        values (:email)
                        """)
                .param("email", author.email())
                .update(keys, "id");
        return new Author(
                keys.getKeyAs(Long.class),
                author.email());
    }

    @Override
    public boolean deleteById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from authors
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Author from the current row. */
    private static Author map(ResultSet rows, int rowNumber) throws SQLException {
        return new Author(
                rows.getLong("id"),
                rows.getString("email"));
    }
}
