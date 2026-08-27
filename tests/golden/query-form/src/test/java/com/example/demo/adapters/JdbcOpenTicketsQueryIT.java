package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import com.example.demo.service.OpenTicketsCriteria;
import com.example.demo.service.OpenTicketsQuery;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcOpenTicketsQueryIT {

    @Autowired
    private TicketRepository repository;

    @Autowired
    private OpenTicketsQuery query;

    @Test
    void filtersInTheRealDatabase() {
        // The stored row, not the argument: with a database-assigned key the
        // two differ by exactly the component the query filters on.
        Ticket stored = repository.save(new Ticket(
                1L,
                "sample",
                Optional.empty()));

        var found = query.execute(new OpenTicketsCriteria(
                Optional.empty()));

        assertThat(found).contains(stored);
    }
}
