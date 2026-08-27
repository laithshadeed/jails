package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcTicketRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcTicketRepositoryIT {

    @Autowired private TicketRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var ticket = repository.save(new Ticket(
                1L,
                "sample"));

        Long key = ticket.id();
        assertThat(repository.findById(key)).contains(ticket);
        assertThat(repository.findAll()).contains(ticket);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
