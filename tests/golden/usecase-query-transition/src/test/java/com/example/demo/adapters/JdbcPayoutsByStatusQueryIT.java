package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.PayoutsByStatusCriteria;
import com.example.demo.service.PayoutsByStatusQuery;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcPayoutsByStatusQueryIT {

    @Autowired
    private PayoutRepository repository;

    @Autowired
    private PayoutsByStatusQuery query;

    @Test
    void filtersInTheRealDatabase() {
        // The stored row, not the argument: with a database-assigned key the
        // two differ by exactly the component the query filters on.
        Payout stored = repository.save(new Payout(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L,
                PayoutStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z")));

        var found = query.execute(new PayoutsByStatusCriteria(
                PayoutStatus.values()[0]));

        assertThat(found).contains(stored);
    }
}
