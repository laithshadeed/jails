package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import com.example.demo.service.MarkNoteSeenCommand;
import com.example.demo.service.MarkNoteSeenUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcMarkNoteSeenTransitionIT {

    @Autowired private NoteRepository repository;
    @Autowired private MarkNoteSeenUseCase useCase;

    @Test
    void appliesOnceAndReportsTheStaleVersionWithoutAnotherMutation() {
        var stored = repository.save(new Note(
                1L,
                "sample",
                true,
                1L));
        var command = new MarkNoteSeenCommand(
                stored.id());

        var applied = useCase.execute(stored.id(), command, 1L);

        assertThat(applied).isInstanceOf(MarkNoteSeenUseCase.Result.Applied.class);
        var resource = ((MarkNoteSeenUseCase.Result.Applied) applied).resource();
        assertThat(resource.version()).isEqualTo(1L + 1);

        // The same expectation a second time is stale, and the outcome
        // carries the row as it now stands rather than a message about it.
        var again = useCase.execute(stored.id(), command, 1L);
        assertThat(again).isInstanceOf(MarkNoteSeenUseCase.Result.StaleVersion.class);
        assertThat(((MarkNoteSeenUseCase.Result.StaleVersion) again).current()).isEqualTo(resource);
        assertThat(repository.findById(stored.id())).contains(resource);
    }

    @Test
    void aCallerThatSendsNoPreconditionAppliesUnconditionallyAndCanRepeat() {
        var stored = repository.save(new Note(
                1L,
                "sample",
                true,
                1L));
        var command = new MarkNoteSeenCommand(
                stored.id());

        // `null` is the absence of a precondition, not a wrong one.
        assertThat(useCase.execute(stored.id(), command, null))
                .isInstanceOf(MarkNoteSeenUseCase.Result.Applied.class);

        // Again: a guarded call would be stale by now, so this is what fails
        // if the guard stops being optional.
        assertThat(useCase.execute(stored.id(), command, null))
                .isInstanceOf(MarkNoteSeenUseCase.Result.Applied.class);
    }
}
