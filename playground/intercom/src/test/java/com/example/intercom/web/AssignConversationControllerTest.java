package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.AssignConversationCommand;
import com.example.intercom.service.AssignConversationUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class AssignConversationControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new AssignConversationController(
            command -> new ConversationAssignment(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    AssignmentStatus.values()[0],
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(AssignConversationController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "workspaceId": "00000000-0000-0000-0000-000000000001",
  "conversationId": "00000000-0000-0000-0000-000000000001",
  "memberId": "00000000-0000-0000-0000-000000000001"
}
"""))
                .hasStatus(201);
    }

}
