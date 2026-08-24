package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.ConversationAssignmentRepository;
import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.AssignmentByConversationQuery;
import com.example.intercom.service.AssignmentByConversationQueryPort;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcAssignmentByConversationQueryIT {

    @Autowired
    private ConversationAssignmentRepository repository;

    @Autowired
    private AssignmentByConversationQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        ConversationAssignment stored = new ConversationAssignment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                AssignmentStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new AssignmentByConversationQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
