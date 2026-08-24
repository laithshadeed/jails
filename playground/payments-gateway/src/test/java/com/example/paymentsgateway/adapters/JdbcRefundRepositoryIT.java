package com.example.paymentsgateway.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.RefundRepository;
import com.example.paymentsgateway.domain.Refund;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcRefundRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcRefundRepositoryIT {

    @Autowired private RefundRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var refund = new Refund(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L,
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(refund);

        String key = String.valueOf(refund.id());
        assertThat(repository.findById(key)).contains(refund);
        assertThat(repository.findAll()).contains(refund);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
