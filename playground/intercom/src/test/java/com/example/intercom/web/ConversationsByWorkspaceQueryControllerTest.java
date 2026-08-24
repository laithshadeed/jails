package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import com.example.intercom.service.ConversationsByWorkspaceQueryPort;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ConversationsByWorkspaceQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new ConversationsByWorkspaceQueryController(
            query -> List.of(new Conversation(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    ConversationStatus.values()[0],
                    Instant.parse("2024-01-01T00:00:00Z"),
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"))),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri(ConversationsByWorkspaceQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "workspaceId": "00000000-0000-0000-0000-000000000001"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
