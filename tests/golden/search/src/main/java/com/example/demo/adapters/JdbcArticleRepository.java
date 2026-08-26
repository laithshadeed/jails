package com.example.demo.adapters;

import com.example.demo.app.ArticleRepository;
import com.example.demo.domain.Article;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/**
 * {@link ArticleRepository} over {@link JdbcClient}. No ORM: the queries are
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
public final class JdbcArticleRepository implements ArticleRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            title,
            body
            """;

    private final JdbcClient db;

    public JdbcArticleRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Article> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from articles
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcArticleRepository::map)
                .optional();
    }

    @Override
    public List<Article> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from articles
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcArticleRepository::map)
                .list();
    }

    @Override
    public void save(Article article) {
        Objects.requireNonNull(article, "article is required");
        db.sql("""
                        insert into articles (id, title, body)
                        values (:id, :title, :body)
                        """)
                .param("id", article.id())
                .param("title", article.title())
                .param("body", article.body())
                .update();
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from articles
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Article from the current row. */
    private static Article map(ResultSet rows, int rowNumber) throws SQLException {
        return new Article(
                rows.getObject("id", UUID.class),
                rows.getString("title"),
                rows.getString("body"));
    }
}
