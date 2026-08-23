package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Message;
import com.example.demo.service.ReceiveMessageCommand;
import com.example.demo.service.ReceiveMessageUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ReceiveMessageControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new ReceiveMessageController(
            command -> new Message(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"))));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(ReceiveMessageController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "body": "sample"
}
"""))
                .hasStatus(201);
    }

}
