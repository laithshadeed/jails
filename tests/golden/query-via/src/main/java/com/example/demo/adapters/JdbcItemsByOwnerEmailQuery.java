package com.example.demo.adapters;

import com.example.demo.domain.Item;
import com.example.demo.service.ItemsByOwnerEmailCriteria;
import com.example.demo.service.ItemsByOwnerEmailQuery;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcItemsByOwnerEmailQuery implements ItemsByOwnerEmailQuery {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 20;

    private static final String COLUMNS =
            """
            items.id,
            items.owner_id,
            items.name,
            items.created_at
            """;

    private final JdbcClient db;

    public JdbcItemsByOwnerEmailQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Item> execute(ItemsByOwnerEmailCriteria criteria) {
        Objects.requireNonNull(criteria, "criteria is required");
        return db.sql("""
                        select %s
                        from items
                        join owners on items.owner_id = owners.id
                        where owners.email = :email
                        order by items.created_at desc, items.name
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("email", criteria.email())
                .param("max_results", MAX_RESULTS)
                .query(JdbcItemsByOwnerEmailQuery::map)
                .list();
    }

    private static Item map(ResultSet rows, int rowNumber) throws SQLException {
        return new Item(
                rows.getObject("id", UUID.class),
                rows.getObject("owner_id", UUID.class),
                rows.getString("name"),
                rows.getObject("created_at", OffsetDateTime.class).toInstant());
    }
}
