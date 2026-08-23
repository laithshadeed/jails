package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.OwnerRepository;
import com.example.demo.domain.Owner;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcOwnerRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcOwnerRepositoryIT {

    @Autowired private OwnerRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var owner = new Owner(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(owner);

        String key = String.valueOf(owner.id());
        assertThat(repository.findById(key)).contains(owner);
        assertThat(repository.findAll()).contains(owner);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
