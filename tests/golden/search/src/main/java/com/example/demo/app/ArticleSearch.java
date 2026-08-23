package com.example.demo.app;

import com.example.demo.domain.Article;
import java.util.List;

/**
 * Full-text search over Article.
 *
 * <p>A port rather than a method on the repository: searching and fetching by
 * id are different questions with different indexes behind them, and a project
 * that later moves search to Elasticsearch replaces this and nothing else.
 */
public interface ArticleSearch {

    /**
     * @param query what the reader typed. It is parsed by PostgreSQL, not
     *     concatenated into SQL -- see the adapter.
     * @param limit how many rows at most. There is no unbounded overload: a
     *     search with no limit is a full scan waiting for the table to grow.
     */
    List<Article> matching(String query, int limit);
}
