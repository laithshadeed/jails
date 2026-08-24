package com.example.webcrawler.adapters;

import com.example.webcrawler.domain.CrawledPage;
import com.example.webcrawler.service.PagesByCrawlRunQuery;
import com.example.webcrawler.service.PagesByCrawlRunQueryPort;
import java.net.URI;
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
public final class JdbcPagesByCrawlRunQuery implements PagesByCrawlRunQueryPort {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
            id,
            crawl_run_id,
            url,
            status_code,
            discovered_at,
            created_at,
            updated_at
            """;

    private final JdbcClient db;

    public JdbcPagesByCrawlRunQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<CrawledPage> execute(PagesByCrawlRunQuery query) {
        Objects.requireNonNull(query, "query is required");
        return db.sql("""
                        select %s
                        from crawled_pages
                        where crawl_run_id = :crawl_run_id
                        order by id
                        limit :max_results
                        """.formatted(COLUMNS))
                .param("crawl_run_id", query.crawlRunId())
                .param("max_results", MAX_RESULTS)
                .query(JdbcPagesByCrawlRunQuery::map)
                .list();
    }

    private static CrawledPage map(ResultSet rows, int rowNumber) throws SQLException {
        return new CrawledPage(
                rows.getObject("id", UUID.class),
                rows.getObject("crawl_run_id", UUID.class),
                URI.create(rows.getString("url")),
                rows.getInt("status_code"),
                rows.getObject("discovered_at", OffsetDateTime.class).toInstant(),
                rows.getObject("created_at", OffsetDateTime.class).toInstant(),
                rows.getObject("updated_at", OffsetDateTime.class).toInstant());
    }
}
