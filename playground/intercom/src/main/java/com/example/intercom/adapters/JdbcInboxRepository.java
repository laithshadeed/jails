package com.example.intercom.adapters;

import com.example.intercom.app.InboxRepository;
import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
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
 * {@link InboxRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcInboxRepository implements InboxRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
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

    public JdbcInboxRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Inbox> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from inboxes
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcInboxRepository::map)
                .optional();
    }

    @Override
    public List<Inbox> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from inboxes
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcInboxRepository::map)
                .list();
    }

    @Override
    public void save(Inbox inbox) {
        Objects.requireNonNull(inbox, "inbox is required");
        db.sql("""
                        insert into inboxes (id, workspace_id, name, channel, created_at, updated_at)
                        values (:id, :workspace_id, :name, :channel, :created_at, :updated_at)
                        """)
                .param("id", inbox.id())
                .param("workspace_id", inbox.workspaceId())
                .param("name", inbox.name())
                .param("channel", inbox.channel().name())
                .param("created_at", Timestamp.from(inbox.createdAt()))
                .param("updated_at", Timestamp.from(inbox.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from inboxes
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Inbox from the current row. */
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
