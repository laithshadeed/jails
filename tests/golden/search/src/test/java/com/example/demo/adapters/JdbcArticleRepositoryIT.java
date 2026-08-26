package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.ArticleRepository;
import com.example.demo.domain.Article;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcArticleRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcArticleRepositoryIT {

    @Autowired private ArticleRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var article = new Article(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                "sample");
        repository.save(article);

        UUID key = article.id();
        assertThat(repository.findById(key)).contains(article);
        assertThat(repository.findAll()).contains(article);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
