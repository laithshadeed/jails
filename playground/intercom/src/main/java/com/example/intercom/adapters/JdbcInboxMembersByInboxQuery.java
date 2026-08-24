package com.example.intercom.adapters;

import com.example.intercom.domain.InboxMember;
import com.example.intercom.service.InboxMembersByInboxQuery;
import com.example.intercom.service.InboxMembersByInboxQueryPort;
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
public final class JdbcInboxMembersByInboxQuery implements InboxMembersByInboxQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            workspace_id,
            inbox_id,
            member_id,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcInboxMembersByInboxQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<InboxMember> execute(InboxMembersByInboxQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from inbox_members
                        where workspace_id = :workspace_id
                          and inbox_id = :inbox_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("inbox_id", query.inboxId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcInboxMembersByInboxQuery::map)
                .list();
    }

    private static InboxMember map(ResultSet rows, int rowNumber) throws SQLException {
        return new InboxMember(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getObject("inbox_id", UUID.class),
                rows.getObject("member_id", UUID.class),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
