package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Item;
import com.example.demo.service.ItemsByOwnerEmailQuery;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ItemsByOwnerEmailQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new ItemsByOwnerEmailQueryController(
            criteria -> List.of(new Item(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z")))));

    @Test
    void postExecutesTheDatabaseQuery() {
        assertThat(mvc.post()
                .uri(ItemsByOwnerEmailQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "email": "sample"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
