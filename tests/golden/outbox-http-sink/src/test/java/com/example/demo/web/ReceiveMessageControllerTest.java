package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Message;
import com.example.demo.service.ReceiveMessageCommand;
import com.example.demo.service.ReceiveMessageUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

@WebMvcTest(ReceiveMessageController.class)
@Import(ReceiveMessageControllerTest.Config.class)
class ReceiveMessageControllerTest {

    @Autowired
    private MockMvcTester mvc;

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

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        ReceiveMessageUseCase useCase() {
            return command -> new Message(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"));
        }

    }
}
