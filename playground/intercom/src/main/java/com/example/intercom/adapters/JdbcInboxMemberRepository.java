package com.example.intercom.adapters;

import com.example.intercom.app.InboxMemberRepository;
import com.example.intercom.domain.InboxMember;
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
 * {@link InboxMemberRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcInboxMemberRepository implements InboxMemberRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
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

    public JdbcInboxMemberRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<InboxMember> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from inbox_members
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcInboxMemberRepository::map)
                .optional();
    }

    @Override
    public List<InboxMember> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from inbox_members
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcInboxMemberRepository::map)
                .list();
    }

    @Override
    public void save(InboxMember inboxMember) {
        Objects.requireNonNull(inboxMember, "inboxMember is required");
        db.sql("""
                        insert into inbox_members (id, workspace_id, inbox_id, member_id, created_at, updated_at)
                        values (:id, :workspace_id, :inbox_id, :member_id, :created_at, :updated_at)
                        """)
                .param("id", inboxMember.id())
                .param("workspace_id", inboxMember.workspaceId())
                .param("inbox_id", inboxMember.inboxId())
                .param("member_id", inboxMember.memberId())
                .param("created_at", Timestamp.from(inboxMember.createdAt()))
                .param("updated_at", Timestamp.from(inboxMember.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from inbox_members
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a InboxMember from the current row. */
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
