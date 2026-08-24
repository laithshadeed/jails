package com.example.webcrawler.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.TestcontainersConfig;
import com.example.webcrawler.app.CrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import java.net.URI;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcCrawledPageRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcCrawledPageRepositoryIT {

    @Autowired private CrawledPageRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var crawledPage = new CrawledPage(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"),
                1,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(crawledPage);

        String key = String.valueOf(crawledPage.id());
        assertThat(repository.findById(key)).contains(crawledPage);
        assertThat(repository.findAll()).contains(crawledPage);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
