package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.adapters.InMemoryNoteRepository;
import com.example.demo.domain.Note;
import org.junit.jupiter.api.Test;

class PostAdminNoteUseCaseTest {

    private final InMemoryNoteRepository repository = new InMemoryNoteRepository();
    private final PostAdminNoteUseCase useCase = new StoringPostAdminNoteUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        PostAdminNoteCommand command = new PostAdminNoteCommand(
                1L,
                "sample");

        Note created = useCase.execute(command);

        assertThat(created.id()).isPositive();
        assertThat(created.authorId()).isEqualTo(command.authorId());
        assertThat(created.body()).isEqualTo(command.body());
        assertThat(repository.findById(created.id())).contains(created);
    }

    /**
     * missing.md M3: two creates are two rows. When the key was
     * constructed rather than assigned, this was two creates and
     * *one* row, with no exception and no log line.
     */
    @Test
    void twoCreatesAreTwoRows() {
        PostAdminNoteCommand command = new PostAdminNoteCommand(
                1L,
                "sample");

        Note first = useCase.execute(command);
        Note second = useCase.execute(command);

        assertThat(second.id()).isNotEqualTo(first.id());
        assertThat(repository.findAll()).hasSize(2);
    }
}
