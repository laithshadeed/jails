package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.WidgetRepository;
import com.example.demo.domain.Widget;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcWidgetRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcWidgetRepositoryIT {

    @Autowired private WidgetRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var widget = repository.save(new Widget(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample"));

        UUID key = widget.id();
        assertThat(repository.findById(key)).contains(widget);
        assertThat(repository.findAll()).contains(widget);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
