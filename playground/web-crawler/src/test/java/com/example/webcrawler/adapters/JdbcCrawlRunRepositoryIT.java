package com.example.webcrawler.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.TestcontainersConfig;
import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import java.net.URI;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcCrawlRunRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcCrawlRunRepositoryIT {

    @Autowired private CrawlRunRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var crawlRun = new CrawlRun(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"),
                CrawlStatus.values()[0],
                1L,
                Optional.empty(),
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(crawlRun);

        String key = String.valueOf(crawlRun.id());
        assertThat(repository.findById(key)).contains(crawlRun);
        assertThat(repository.findAll()).contains(crawlRun);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
