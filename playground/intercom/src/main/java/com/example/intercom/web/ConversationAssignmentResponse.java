package com.example.intercom.web;

import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import java.time.Instant;
import java.util.UUID;

/**
 * What this application returns. Deliberately not ConversationAssignment itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record ConversationAssignmentResponse(
        UUID id,
        UUID workspaceId,
        UUID conversationId,
        UUID memberId,
        AssignmentStatus status,
        Long version,
        Instant assignedAt,
        Instant createdAt,
        Instant updatedAt) {

    /** @return the response describing {@code conversationAssignment}. */
    public static ConversationAssignmentResponse from(ConversationAssignment conversationAssignment) {
        return new ConversationAssignmentResponse(
                conversationAssignment.id(),
                conversationAssignment.workspaceId(),
                conversationAssignment.conversationId(),
                conversationAssignment.memberId(),
                conversationAssignment.status(),
                conversationAssignment.version(),
                conversationAssignment.assignedAt(),
                conversationAssignment.createdAt(),
                conversationAssignment.updatedAt());
    }
}
