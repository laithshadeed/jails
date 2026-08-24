package com.example.webcrawler.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.domain.CrawledPage;
import com.example.webcrawler.service.RecordCrawledPageCommand;
import com.example.webcrawler.service.RecordCrawledPageUseCase;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class RecordCrawledPageControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new RecordCrawledPageController(
            command -> new CrawledPage(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    URI.create("https://example.com"),
                    1,
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"))));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(RecordCrawledPageController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "crawlRunId": "00000000-0000-0000-0000-000000000001",
  "url": "https://example.test/items/1",
  "statusCode": 7
}
"""))
                .hasStatus(201);
    }

}
