package com.example.intercom.adapters;

import com.example.intercom.app.MessageRepository;
import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
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
 * {@link MessageRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcMessageRepository implements MessageRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            workspace_id,
            conversation_id,
            direction,
            body,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcMessageRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Message> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from messages
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcMessageRepository::map)
                .optional();
    }

    @Override
    public List<Message> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from messages
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcMessageRepository::map)
                .list();
    }

    @Override
    public void save(Message message) {
        Objects.requireNonNull(message, "message is required");
        db.sql("""
                        insert into messages (id, workspace_id, conversation_id, direction, body, created_at, updated_at)
                        values (:id, :workspace_id, :conversation_id, :direction, :body, :created_at, :updated_at)
                        """)
                .param("id", message.id())
                .param("workspace_id", message.workspaceId())
                .param("conversation_id", message.conversationId())
                .param("direction", message.direction().name())
                .param("body", message.body())
                .param("created_at", Timestamp.from(message.createdAt()))
                .param("updated_at", Timestamp.from(message.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from messages
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Message from the current row. */
    private static Message map(ResultSet rows, int rowNumber) throws SQLException {
        return new Message(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getObject("conversation_id", UUID.class),
                MessageDirection.valueOf(rows.getString("direction")),
                rows.getString("body"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
