package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Workspace;
import com.example.intercom.service.CreateWorkspaceCommand;
import com.example.intercom.service.CreateWorkspaceUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class CreateWorkspaceControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new CreateWorkspaceController(
            command -> new Workspace(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(CreateWorkspaceController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "name": "sample"
}
"""))
                .hasStatus(201);
    }

}
