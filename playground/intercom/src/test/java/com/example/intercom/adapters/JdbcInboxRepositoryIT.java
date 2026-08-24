package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.InboxRepository;
import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcInboxRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcInboxRepositoryIT {

    @Autowired private InboxRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var inbox = new Inbox(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                InboxChannel.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(inbox);

        String key = String.valueOf(inbox.id());
        assertThat(repository.findById(key)).contains(inbox);
        assertThat(repository.findAll()).contains(inbox);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
