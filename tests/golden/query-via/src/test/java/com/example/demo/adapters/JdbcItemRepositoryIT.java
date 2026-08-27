package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcItemRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcItemRepositoryIT {

    @Autowired private ItemRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var item = repository.save(new Item(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Instant.parse("2024-01-01T00:00:00Z")));

        UUID key = item.id();
        assertThat(repository.findById(key)).contains(item);
        assertThat(repository.findAll()).contains(item);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
