package com.example.intercom.adapters;

import com.example.intercom.app.ConversationAssignmentRepository;
import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
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
 * {@link ConversationAssignmentRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcConversationAssignmentRepository implements ConversationAssignmentRepository {

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
            member_id,
            status,
            version,
            assigned_at,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcConversationAssignmentRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<ConversationAssignment> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from conversation_assignments
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcConversationAssignmentRepository::map)
                .optional();
    }

    @Override
    public List<ConversationAssignment> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from conversation_assignments
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcConversationAssignmentRepository::map)
                .list();
    }

    @Override
    public void save(ConversationAssignment conversationAssignment) {
        Objects.requireNonNull(conversationAssignment, "conversationAssignment is required");
        db.sql("""
                        insert into conversation_assignments (id, workspace_id, conversation_id, member_id, status, version, assigned_at, created_at, updated_at)
                        values (:id, :workspace_id, :conversation_id, :member_id, :status, :version, :assigned_at, :created_at, :updated_at)
                        """)
                .param("id", conversationAssignment.id())
                .param("workspace_id", conversationAssignment.workspaceId())
                .param("conversation_id", conversationAssignment.conversationId())
                .param("member_id", conversationAssignment.memberId())
                .param("status", conversationAssignment.status().name())
                .param("version", conversationAssignment.version())
                .param("assigned_at", Timestamp.from(conversationAssignment.assignedAt()))
                .param("created_at", Timestamp.from(conversationAssignment.createdAt()))
                .param("updated_at", Timestamp.from(conversationAssignment.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from conversation_assignments
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a ConversationAssignment from the current row. */
    private static ConversationAssignment map(ResultSet rows, int rowNumber) throws SQLException {
        return new ConversationAssignment(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getObject("conversation_id", UUID.class),
                rows.getObject("member_id", UUID.class),
                AssignmentStatus.valueOf(rows.getString("status")),
                rows.getLong("version"),
                rows.getObject("assigned_at", OffsetDateTime.class).toInstant(),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
