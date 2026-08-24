package com.example.webcrawler.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.domain.CrawledPage;
import com.example.webcrawler.service.PagesByCrawlRunQueryPort;
import java.net.URI;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class PagesByCrawlRunQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new PagesByCrawlRunQueryController(
            query -> List.of(new CrawledPage(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    URI.create("https://example.com"),
                    1,
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")))));

    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri(PagesByCrawlRunQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "crawlRunId": "00000000-0000-0000-0000-000000000001"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
