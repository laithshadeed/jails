package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.AuthorRepository;
import com.example.demo.domain.Author;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcAuthorRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcAuthorRepositoryIT {

    @Autowired private AuthorRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var author = repository.save(new Author(
                1L,
                "sample"));

        Long key = author.id();
        assertThat(repository.findById(key)).contains(author);
        assertThat(repository.findAll()).contains(author);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
