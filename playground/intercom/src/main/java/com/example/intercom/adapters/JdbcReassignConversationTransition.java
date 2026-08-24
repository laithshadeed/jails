package com.example.intercom.adapters;

import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.ReassignConversationCommand;
import com.example.intercom.service.ReassignConversationUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap: scoped matches cannot mutate another tenant's row. */
@Component
public class JdbcReassignConversationTransition implements ReassignConversationUseCase {

    private final JdbcClient db;

    public JdbcReassignConversationTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public ConversationAssignment execute(ReassignConversationCommand command) {
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update conversation_assignments
                        set member_id = :member_id,
                            status = :status,
                            updated_at = current_timestamp,
                            version = version + 1
                        where id = :id
                          and workspace_id = :workspace_id
                          and version = :version
                        returning id, workspace_id, conversation_id, member_id, status, version, assigned_at, created_at, updated_at
                        """)
                .param("id", command.id())
                .param("workspace_id", command.workspaceId())
                .param("member_id", command.memberId())
                .param("status", command.status().name())
                .param("version", command.version())
                .query(JdbcReassignConversationTransition::map)
                .optional();
        if (updated.isPresent()) return updated.orElseThrow();

        boolean existsInScope = db.sql("""
                        select exists(
                            select 1 from conversation_assignments
                            where id = :id
                                  and workspace_id = :workspace_id
                        )
                        """)
                .param("id", command.id())
                .param("workspace_id", command.workspaceId())
                .query(Boolean.class)
                .single();
        if (existsInScope) throw new ReassignConversationUseCase.StaleVersionException();
        throw new ReassignConversationUseCase.NotFoundException();
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
