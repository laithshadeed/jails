package com.example.webcrawler.adapters;

import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
import com.example.webcrawler.service.CrawlRunsByStatusQuery;
import com.example.webcrawler.service.CrawlRunsByStatusQueryPort;
import java.net.URI;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class JdbcCrawlRunsByStatusQuery implements CrawlRunsByStatusQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            seed_url,
            status,
            pages_visited,
            started_at,
            finished_at,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcCrawlRunsByStatusQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<CrawlRun> execute(CrawlRunsByStatusQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from crawl_runs
                        where status = :status
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("status", query.status().name())
                .param("max_results", MAX_RESULTS)
                .query(JdbcCrawlRunsByStatusQuery::map)
                .list();
    }

    private static CrawlRun map(ResultSet rows, int rowNumber) throws SQLException {
        return new CrawlRun(
                rows.getObject("id", UUID.class),
                URI.create(rows.getString("seed_url")),
                CrawlStatus.valueOf(rows.getString("status")),
                rows.getLong("pages_visited"),
                Optional.ofNullable(rows.getObject("started_at", OffsetDateTime.class)).map(OffsetDateTime::toInstant),
                Optional.ofNullable(rows.getObject("finished_at", OffsetDateTime.class)).map(OffsetDateTime::toInstant),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
