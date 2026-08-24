package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.ContactRepository;
import com.example.intercom.domain.Contact;
import com.example.intercom.service.ContactsByWorkspaceQuery;
import com.example.intercom.service.ContactsByWorkspaceQueryPort;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcContactsByWorkspaceQueryIT {

    @Autowired
    private ContactRepository repository;

    @Autowired
    private ContactsByWorkspaceQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        Contact stored = new Contact(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);

        var found = queryPort.execute(new ContactsByWorkspaceQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
