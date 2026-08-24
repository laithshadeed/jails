package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.InboxMemberRepository;
import com.example.intercom.domain.InboxMember;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcInboxMemberRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcInboxMemberRepositoryIT {

    @Autowired private InboxMemberRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var inboxMember = new InboxMember(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(inboxMember);

        String key = String.valueOf(inboxMember.id());
        assertThat(repository.findById(key)).contains(inboxMember);
        assertThat(repository.findAll()).contains(inboxMember);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
