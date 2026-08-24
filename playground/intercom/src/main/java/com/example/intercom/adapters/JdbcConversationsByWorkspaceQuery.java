package com.example.intercom.adapters;

import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import com.example.intercom.service.ConversationsByWorkspaceQuery;
import com.example.intercom.service.ConversationsByWorkspaceQueryPort;
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
public final class JdbcConversationsByWorkspaceQuery implements ConversationsByWorkspaceQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

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

    public JdbcConversationsByWorkspaceQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Conversation> execute(ConversationsByWorkspaceQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from conversations
                        where workspace_id = :workspace_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcConversationsByWorkspaceQuery::map)
                .list();
    }

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
