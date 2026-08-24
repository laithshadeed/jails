package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.MemberRepository;
import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcMemberRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcMemberRepositoryIT {

    @Autowired private MemberRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var member = new Member(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                "sample",
                MemberRole.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(member);

        String key = String.valueOf(member.id());
        assertThat(repository.findById(key)).contains(member);
        assertThat(repository.findAll()).contains(member);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
