package com.example.paymentsgateway.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
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
 * in {@link JdbcPaymentRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcPaymentRepositoryIT {

    @Autowired private PaymentRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var payment = new Payment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0],
                PaymentStatus.values()[0],
                1L,
                Optional.empty(),
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(payment);

        String key = String.valueOf(payment.id());
        assertThat(repository.findById(key)).contains(payment);
        assertThat(repository.findAll()).contains(payment);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
