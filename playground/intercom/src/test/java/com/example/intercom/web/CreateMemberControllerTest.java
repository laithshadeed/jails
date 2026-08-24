package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import com.example.intercom.service.CreateMemberCommand;
import com.example.intercom.service.CreateMemberUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class CreateMemberControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new CreateMemberController(
            command -> new Member(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    "sample",
                    MemberRole.values()[0],
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(CreateMemberController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "workspaceId": "00000000-0000-0000-0000-000000000001",
  "email": "sample",
  "displayName": "sample",
  "role": "OWNER"
}
"""))
                .hasStatus(201);
    }

}
