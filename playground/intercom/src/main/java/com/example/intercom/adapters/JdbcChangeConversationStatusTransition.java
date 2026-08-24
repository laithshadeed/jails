package com.example.intercom.adapters;

import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import com.example.intercom.service.ChangeConversationStatusCommand;
import com.example.intercom.service.ChangeConversationStatusUseCase;
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
public class JdbcChangeConversationStatusTransition implements ChangeConversationStatusUseCase {

    private final JdbcClient db;

    public JdbcChangeConversationStatusTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public Conversation execute(ChangeConversationStatusCommand command) {
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update conversations
                        set status = :status,
                            updated_at = current_timestamp,
                            version = version + 1
                        where id = :id
                          and workspace_id = :workspace_id
                          and version = :version
                        returning id, workspace_id, contact_id, inbox_id, status, last_message_at, version, created_at, updated_at
                        """)
                .param("id", command.id())
                .param("workspace_id", command.workspaceId())
                .param("status", command.status().name())
                .param("version", command.version())
                .query(JdbcChangeConversationStatusTransition::map)
                .optional();
        if (updated.isPresent()) return updated.orElseThrow();

        boolean existsInScope = db.sql("""
                        select exists(
                            select 1 from conversations
                            where id = :id
                                  and workspace_id = :workspace_id
                        )
                        """)
                .param("id", command.id())
                .param("workspace_id", command.workspaceId())
                .query(Boolean.class)
                .single();
        if (existsInScope) throw new ChangeConversationStatusUseCase.StaleVersionException();
        throw new ChangeConversationStatusUseCase.NotFoundException();
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
