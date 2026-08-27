package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.TopicRepository;
import com.example.demo.domain.Topic;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcTopicRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcTopicRepositoryIT {

    @Autowired private TopicRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var topic = repository.save(new Topic(
                1L,
                1L,
                "sample",
                1L));

        Long key = topic.id();
        assertThat(repository.findById(key)).contains(topic);
        assertThat(repository.findAll()).contains(topic);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
