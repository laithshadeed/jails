package com.example.paymentsgateway.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.MerchantRepository;
import com.example.paymentsgateway.domain.Merchant;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcMerchantRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcMerchantRepositoryIT {

    @Autowired private MerchantRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var merchant = new Merchant(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                "sample",
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(merchant);

        String key = String.valueOf(merchant.id());
        assertThat(repository.findById(key)).contains(merchant);
        assertThat(repository.findAll()).contains(merchant);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
