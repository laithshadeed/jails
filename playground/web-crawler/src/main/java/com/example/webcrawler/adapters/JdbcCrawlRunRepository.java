package com.example.webcrawler.adapters;

import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import com.example.webcrawler.domain.CrawlStatus;
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

/**
 * {@link CrawlRunRepository} over {@link JdbcClient}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {@code ?} list
 * is a silent-swap bug waiting for a schema change -- reorder two columns of
 * the same type and nothing fails to compile and nothing throws.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
@Component
public final class JdbcCrawlRunRepository implements CrawlRunRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
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

    public JdbcCrawlRunRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<CrawlRun> findById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from crawl_runs
                        where id = cast(:id as uuid)
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcCrawlRunRepository::map)
                .optional();
    }

    @Override
    public List<CrawlRun> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from crawl_runs
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcCrawlRunRepository::map)
                .list();
    }

    @Override
    public void save(CrawlRun crawlRun) {
        Objects.requireNonNull(crawlRun, "crawlRun is required");
        db.sql("""
                        insert into crawl_runs (id, seed_url, status, pages_visited, started_at, finished_at, created_at, updated_at)
                        values (:id, :seed_url, :status, :pages_visited, :started_at, :finished_at, :created_at, :updated_at)
                        """)
                .param("id", crawlRun.id())
                .param("seed_url", crawlRun.seedUrl().toString())
                .param("status", crawlRun.status().name())
                .param("pages_visited", crawlRun.pagesVisited())
                .param("started_at", crawlRun.startedAt().map(Timestamp::from).orElse(null))
                .param("finished_at", crawlRun.finishedAt().map(Timestamp::from).orElse(null))
                .param("created_at", Timestamp.from(crawlRun.createdAt()))
                .param("updated_at", Timestamp.from(crawlRun.updatedAt()))
                .update();
    }

    @Override
    public boolean deleteById(String id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from crawl_runs
                        where id = cast(:id as uuid)
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a CrawlRun from the current row. */
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
