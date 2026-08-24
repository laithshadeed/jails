package com.example.intercom.adapters;

import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import com.example.intercom.service.MessagesByConversationQuery;
import com.example.intercom.service.MessagesByConversationQueryPort;
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
public final class JdbcMessagesByConversationQuery implements MessagesByConversationQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

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

    public JdbcMessagesByConversationQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Message> execute(MessagesByConversationQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from messages
                        where workspace_id = :workspace_id
                          and conversation_id = :conversation_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("conversation_id", query.conversationId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcMessagesByConversationQuery::map)
                .list();
    }

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
