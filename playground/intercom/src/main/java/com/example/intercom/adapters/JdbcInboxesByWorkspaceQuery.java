package com.example.intercom.adapters;

import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
import com.example.intercom.service.InboxesByWorkspaceQuery;
import com.example.intercom.service.InboxesByWorkspaceQueryPort;
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
public final class JdbcInboxesByWorkspaceQuery implements InboxesByWorkspaceQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            workspace_id,
            name,
            channel,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcInboxesByWorkspaceQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Inbox> execute(InboxesByWorkspaceQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from inboxes
                        where workspace_id = :workspace_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcInboxesByWorkspaceQuery::map)
                .list();
    }

    private static Inbox map(ResultSet rows, int rowNumber) throws SQLException {
        return new Inbox(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getString("name"),
                InboxChannel.valueOf(rows.getString("channel")),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
