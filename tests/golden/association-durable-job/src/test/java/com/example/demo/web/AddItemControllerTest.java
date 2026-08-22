package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Item;
import com.example.demo.service.AddItemCommand;
import com.example.demo.service.AddItemUseCase;
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

@WebMvcTest(AddItemController.class)
@Import(AddItemControllerTest.Config.class)
class AddItemControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(AddItemController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "ownerId": "00000000-0000-0000-0000-000000000001",
  "name": "sample"
}
"""))
                .hasStatus(201);
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        AddItemUseCase useCase() {
            return command -> new Item(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"));
        }

    }
}
