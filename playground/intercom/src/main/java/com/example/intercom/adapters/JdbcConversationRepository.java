package com.example.intercom.adapters;

import com.example.intercom.app.ConversationRepository;
import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
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
 * {@link ConversationRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcConversationRepository implements ConversationRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            workspace_id,
            contact_id,
            inbox_id,
            status,
            last_message_at,
            version,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcConversationRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Conversation> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from conversations
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcConversationRepository::map)
                .optional();
    }

    @Override
    public List<Conversation> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from conversations
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcConversationRepository::map)
                .list();
    }

    @Override
    public void save(Conversation conversation) {
        Objects.requireNonNull(conversation, "conversation is required");
        db.sql("""
                        insert into conversations (id, workspace_id, contact_id, inbox_id, status, last_message_at, version, created_at, updated_at)
                        values (:id, :workspace_id, :contact_id, :inbox_id, :status, :last_message_at, :version, :created_at, :updated_at)
                        """)
                .param("id", conversation.id())
                .param("workspace_id", conversation.workspaceId())
                .param("contact_id", conversation.contactId())
                .param("inbox_id", conversation.inboxId())
                .param("status", conversation.status().name())
                .param("last_message_at", Timestamp.from(conversation.lastMessageAt()))
                .param("version", conversation.version())
                .param("created_at", Timestamp.from(conversation.createdAt()))
                .param("updated_at", Timestamp.from(conversation.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from conversations
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Conversation from the current row. */
    private static Conversation map(ResultSet rows, int rowNumber) throws SQLException {
        return new Conversation(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getObject("contact_id", UUID.class),
                rows.getObject("inbox_id", UUID.class),
                ConversationStatus.valueOf(rows.getString("status")),
                rows.getObject("last_message_at", OffsetDateTime.class).toInstant(),
                rows.getLong("version"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
