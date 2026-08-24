package com.example.webcrawler.adapters;

import com.example.webcrawler.app.CrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link CrawledPageRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcCrawledPageRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryCrawledPageRepository implements CrawledPageRepository {

    private final Map<String, CrawledPage> items = new ConcurrentHashMap<>();

    @Override
    public Optional<CrawledPage> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<CrawledPage> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(CrawledPage crawledPage) {
        items.put(String.valueOf(crawledPage.id()), crawledPage);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
