package com.example.demo.adapters;

import com.example.demo.app.TopicRepository;
import com.example.demo.domain.Topic;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.jdbc.support.GeneratedKeyHolder;
import org.springframework.stereotype.Component;

/**
 * {@link TopicRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcTopicRepository implements TopicRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            user_id,
            subject,
            version
            """;

    private final JdbcClient db;

    public JdbcTopicRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Topic> findById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from topics
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcTopicRepository::map)
                .optional();
    }

    @Override
    public List<Topic> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by.
        return db.sql("""
                        select %s
                        from topics
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcTopicRepository::map)
                .list();
    }

    @Override
    public Topic save(Topic topic) {
        Objects.requireNonNull(topic, "topic is required");
        var keys = new GeneratedKeyHolder();
        db.sql("""
                        insert into topics (user_id, subject, version)
                        values (:user_id, :subject, :version)
                        """)
                .param("user_id", topic.userId())
                .param("subject", topic.subject())
                .param("version", topic.version())
                .update(keys, "id");
        return new Topic(
                keys.getKeyAs(Long.class),
                topic.userId(),
                topic.subject(),
                topic.version());
    }

    @Override
    public boolean deleteById(Long id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from topics
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Topic from the current row. */
    private static Topic map(ResultSet rows, int rowNumber) throws SQLException {
        return new Topic(
                rows.getLong("id"),
                rows.getLong("user_id"),
                rows.getString("subject"),
                rows.getLong("version"));
    }
}
