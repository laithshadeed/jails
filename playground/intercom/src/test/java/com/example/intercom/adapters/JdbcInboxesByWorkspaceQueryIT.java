package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.InboxRepository;
import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
import com.example.intercom.service.InboxesByWorkspaceQuery;
import com.example.intercom.service.InboxesByWorkspaceQueryPort;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcInboxesByWorkspaceQueryIT {

    @Autowired
    private InboxRepository repository;

    @Autowired
    private InboxesByWorkspaceQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        Inbox stored = new Inbox(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                InboxChannel.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new InboxesByWorkspaceQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
