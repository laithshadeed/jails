package com.example.demo.adapters;

import com.example.demo.app.ArticleRepository;
import com.example.demo.domain.Article;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link ArticleRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
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

    private final Map<String, Article> items = new ConcurrentHashMap<>();

    @Override
    public Optional<Article> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<Article> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(Article article) {
        items.put(String.valueOf(article.id()), article);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
