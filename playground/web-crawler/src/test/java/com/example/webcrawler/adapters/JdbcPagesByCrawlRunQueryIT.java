package com.example.webcrawler.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.TestcontainersConfig;
import com.example.webcrawler.app.CrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import com.example.webcrawler.service.PagesByCrawlRunQuery;
import com.example.webcrawler.service.PagesByCrawlRunQueryPort;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcPagesByCrawlRunQueryIT {

    @Autowired
    private CrawledPageRepository repository;

    @Autowired
    private PagesByCrawlRunQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        CrawledPage stored = new CrawledPage(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"),
                1,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new PagesByCrawlRunQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
