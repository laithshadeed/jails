package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.MemberRepository;
import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import com.example.intercom.service.MembersByWorkspaceQuery;
import com.example.intercom.service.MembersByWorkspaceQueryPort;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcMembersByWorkspaceQueryIT {

    @Autowired
    private MemberRepository repository;

    @Autowired
    private MembersByWorkspaceQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        Member stored = new Member(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                "sample",
                MemberRole.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new MembersByWorkspaceQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
