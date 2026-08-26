package com.example.demo.adapters;

import com.example.demo.app.ArticleRepository;
import com.example.demo.domain.Article;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link ArticleRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the {@code id} component -- the same one the JDBC
 * adapter's {@code where} clause uses.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcArticleRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryArticleRepository implements ArticleRepository {

    private final Map<UUID, Article> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Article> findById(UUID id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Article> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Article article) {
        items.put(article.id(), article);
    }

    @Override
    public boolean deleteById(UUID id) {
        return items.remove(id) != null;
    }
}
