package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcNoteRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class JdbcNoteRepositoryIT {

    @Autowired private NoteRepository repository;

    @Test
    void roundTripsThroughTheRealDatabase() {
        var note = repository.save(new Note(
                1L,
                "sample",
                true,
                1L));

        Long key = note.id();
        assertThat(repository.findById(key)).contains(note);
        assertThat(repository.findAll()).contains(note);

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();
    }
}
