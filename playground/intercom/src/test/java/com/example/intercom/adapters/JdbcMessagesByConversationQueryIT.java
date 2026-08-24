package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.MessageRepository;
import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import com.example.intercom.service.MessagesByConversationQuery;
import com.example.intercom.service.MessagesByConversationQueryPort;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcMessagesByConversationQueryIT {

    @Autowired
    private MessageRepository repository;

    @Autowired
    private MessagesByConversationQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        Message stored = new Message(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                MessageDirection.values()[0],
                "sample",
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new MessagesByConversationQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
