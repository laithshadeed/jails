package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.MessageRepository;
import com.example.demo.domain.Message;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcMessageRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcMessageRepositoryIT {

    @Autowired private MessageRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var message = new Message(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(message);

        UUID key = message.id();
        assertThat(repository.findById(key)).contains(message);
        assertThat(repository.findAll()).contains(message);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
