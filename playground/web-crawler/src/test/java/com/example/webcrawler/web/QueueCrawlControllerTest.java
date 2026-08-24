package com.example.webcrawler.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import com.example.webcrawler.service.QueueCrawlCommand;
import com.example.webcrawler.service.QueueCrawlUseCase;
import java.net.URI;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class QueueCrawlControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new QueueCrawlController(
            command -> new CrawlRun(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    URI.create("https://example.com"),
                    CrawlStatus.values()[0],
                    1L,
                    Optional.empty(),
                    Optional.empty(),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"))));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(QueueCrawlController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "seedUrl": "https://example.test/items/1"
}
"""))
                .hasStatus(201);
    }

}
