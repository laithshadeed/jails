package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import com.example.intercom.service.MessagesByConversationQueryPort;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class MessagesByConversationQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new MessagesByConversationQueryController(
            query -> List.of(new Message(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    MessageDirection.values()[0],
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"))),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri(MessagesByConversationQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "workspaceId": "00000000-0000-0000-0000-000000000001",
  "conversationId": "00000000-0000-0000-0000-000000000001"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
