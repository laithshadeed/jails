package com.example.demo.adapters;

import com.example.demo.domain.Ticket;
import com.example.demo.service.OpenTicketsCriteria;
import com.example.demo.service.OpenTicketsQuery;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcOpenTicketsQuery implements OpenTicketsQuery {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            subject,
            status
            """;

    private final JdbcClient db;

    public JdbcOpenTicketsQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Ticket> execute(OpenTicketsCriteria criteria) {
        Objects.requireNonNull(criteria, "criteria is required");
        return db.sql("""
                        select %s
                        from tickets
                        where (cast(:status as text) is null or status = :status)
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("status", criteria.status().orElse(null))
                .param("max_results", MAX_RESULTS)
                .query(JdbcOpenTicketsQuery::map)
                .list();
    }

    private static Ticket map(ResultSet rows, int rowNumber) throws SQLException {
        return new Ticket(
                rows.getLong("id"),
                rows.getString("subject"),
                Optional.ofNullable(rows.getString("status")));
    }
}
