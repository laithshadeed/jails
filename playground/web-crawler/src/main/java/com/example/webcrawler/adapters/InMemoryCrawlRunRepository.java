package com.example.webcrawler.adapters;

import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * {@link CrawlRunRepository} in memory, so the application runs before it has
 * a database.
 *
 * <p>Keyed on the record's own {@code id} component.
 *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
 * <p>Not a bean: this project has a {@code DataSource}, so {@code JdbcCrawlRunRepository}
 * is the {@code @Component}. This stays as a fake for tests that want a
 * repository without a container -- construct it directly.
 */
public class InMemoryCrawlRunRepository implements CrawlRunRepository {

    private final Map<String, CrawlRun> items = new ConcurrentHashMap<>();

    @Override
    public Optional<CrawlRun> findById(String id) {
        return Optional.ofNullable(items.get(id));
    }

    @Override
    public List<CrawlRun> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save(CrawlRun crawlRun) {
        items.put(String.valueOf(crawlRun.id()), crawlRun);
    }

    @Override
    public boolean deleteById(String id) {
        return items.remove(id) != null;
    }
}
