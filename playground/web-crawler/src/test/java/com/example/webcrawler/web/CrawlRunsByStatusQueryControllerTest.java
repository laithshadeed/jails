package com.example.webcrawler.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import com.example.webcrawler.service.CrawlRunsByStatusQueryPort;
import java.net.URI;
import java.time.Instant;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class CrawlRunsByStatusQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new CrawlRunsByStatusQueryController(
            query -> List.of(new CrawlRun(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    URI.create("https://example.com"),
                    CrawlStatus.values()[0],
                    1L,
                    Optional.empty(),
                    Optional.empty(),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")))));

    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri(CrawlRunsByStatusQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "status": "QUEUED"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
