package com.example.intercom.web;

import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Conversation itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record ConversationResponse(
        UUID id,
        UUID workspaceId,
        UUID contactId,
        UUID inboxId,
        ConversationStatus status,
        Instant lastMessageAt,
        Long version,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code conversation}. */
    public static ConversationResponse from(Conversation conversation) {
        return new ConversationResponse(
                conversation.id(),
                conversation.workspaceId(),
                conversation.contactId(),
                conversation.inboxId(),
                conversation.status(),
                conversation.lastMessageAt(),
                conversation.version(),
                conversation.createdAt(),
                conversation.updatedAt());
    }
}
