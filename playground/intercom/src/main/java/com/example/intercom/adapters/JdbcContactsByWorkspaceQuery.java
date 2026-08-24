package com.example.intercom.adapters;

import com.example.intercom.domain.Contact;
import com.example.intercom.service.ContactsByWorkspaceQuery;
import com.example.intercom.service.ContactsByWorkspaceQueryPort;
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

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcContactsByWorkspaceQuery implements ContactsByWorkspaceQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            workspace_id,
            email,
            display_name,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcContactsByWorkspaceQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Contact> execute(ContactsByWorkspaceQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from contacts
                        where workspace_id = :workspace_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("workspace_id", query.workspaceId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcContactsByWorkspaceQuery::map)
                .list();
    }

    private static Contact map(ResultSet rows, int rowNumber) throws SQLException {
        return new Contact(
                rows.getObject("id", UUID.class),
                rows.getObject("workspace_id", UUID.class),
                rows.getString("email"),
                Optional.ofNullable(rows.getString("display_name")),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
