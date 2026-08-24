package com.example.webcrawler.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.TestcontainersConfig;
import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import com.example.webcrawler.service.CrawlRunsByStatusQuery;
import com.example.webcrawler.service.CrawlRunsByStatusQueryPort;
import java.net.URI;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcCrawlRunsByStatusQueryIT {

    @Autowired
    private CrawlRunRepository repository;

    @Autowired
    private CrawlRunsByStatusQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        CrawlRun stored = new CrawlRun(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"),
                CrawlStatus.values()[0],
                1L,
                Optional.empty(),
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new CrawlRunsByStatusQuery(
                CrawlStatus.values()[0]));

        assertThat(found).contains(stored);
    }
}
