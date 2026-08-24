package com.example.paymentsgateway.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import com.example.paymentsgateway.service.PaymentsByMerchantQuery;
import com.example.paymentsgateway.service.PaymentsByMerchantQueryPort;
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
class JdbcPaymentsByMerchantQueryIT {

    @Autowired
    private PaymentRepository repository;

    @Autowired
    private PaymentsByMerchantQueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        Payment stored = new Payment(
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
        repository.save(stored);

        var found = queryPort.execute(new PaymentsByMerchantQuery(
                UUID.fromString("00000000-0000-0000-0000-000000000001")));

        assertThat(found).contains(stored);
    }
}
