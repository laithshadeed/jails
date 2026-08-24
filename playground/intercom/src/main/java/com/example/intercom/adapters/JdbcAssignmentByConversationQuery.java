package com.example.intercom.adapters;

import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.AssignmentByConversationQuery;
import com.example.intercom.service.AssignmentByConversationQueryPort;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcAssignmentByConversationQuery implements AssignmentByConversationQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

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

    public JdbcAssignmentByConversationQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<ConversationAssignment> execute(AssignmentByConversationQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from conversation_assignments
                        where workspace_id = :workspace_id
                          and conversation_id = :conversation_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("conversation_id", query.conversationId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcAssignmentByConversationQuery::map)
                .list();
    }

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
