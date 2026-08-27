package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.ItemRepository;
import com.example.demo.app.OwnerRepository;
import com.example.demo.domain.Item;
import com.example.demo.domain.Owner;
import com.example.demo.service.ItemsByOwnerEmailCriteria;
import com.example.demo.service.ItemsByOwnerEmailQuery;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcItemsByOwnerEmailQueryIT {

    @Autowired
    private ItemRepository repository;

    @Autowired
    private OwnerRepository parents;

    @Autowired
    private ItemsByOwnerEmailQuery query;

    @Test
    void filtersInTheRealDatabase() {
        Owner parent = parents.save(new Owner(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Instant.parse("2024-01-01T00:00:00Z")));
        // The stored row, not the argument: with a database-assigned key the
        // two differ by exactly the component the query filters on.
        Item stored = repository.save(new Item(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                parent.id(),
                "sample",
                Instant.parse("2024-01-01T00:00:00Z")));

        var found = query.execute(new ItemsByOwnerEmailCriteria(
                parent.email()));

        assertThat(found).contains(stored);
    }
}
