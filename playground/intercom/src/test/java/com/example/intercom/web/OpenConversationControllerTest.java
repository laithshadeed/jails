package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import com.example.intercom.service.OpenConversationCommand;
import com.example.intercom.service.OpenConversationUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class OpenConversationControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new OpenConversationController(
            command -> new Conversation(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    ConversationStatus.values()[0],
                    Instant.parse("2024-01-01T00:00:00Z"),
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(OpenConversationController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "workspaceId": "00000000-0000-0000-0000-000000000001",
  "contactId": "00000000-0000-0000-0000-000000000001",
  "inboxId": "00000000-0000-0000-0000-000000000001"
}
"""))
                .hasStatus(201);
    }

}
