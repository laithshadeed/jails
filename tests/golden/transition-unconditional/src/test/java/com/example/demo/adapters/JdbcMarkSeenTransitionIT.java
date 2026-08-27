package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import com.example.demo.service.MarkSeenCommand;
import com.example.demo.service.MarkSeenUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcMarkSeenTransitionIT {

    @Autowired private NoteRepository repository;
    @Autowired private MarkSeenUseCase useCase;

    @Test
    void appliesOnceAndReportsTheStaleVersionWithoutAnotherMutation() {
        var stored = repository.save(new Note(
                1L,
                "sample",
                true,
                1L));
        var command = new MarkSeenCommand(
                stored.id());

        var applied = useCase.execute(stored.id(), command, 1L);

        assertThat(applied).isInstanceOf(MarkSeenUseCase.Result.Applied.class);
        var resource = ((MarkSeenUseCase.Result.Applied) applied).resource();
        assertThat(resource.version()).isEqualTo(1L + 1);

        // The same expectation a second time is stale, and the outcome
        // carries the row as it now stands rather than a message about it.
        var again = useCase.execute(stored.id(), command, 1L);
        assertThat(again).isInstanceOf(MarkSeenUseCase.Result.StaleVersion.class);
        assertThat(((MarkSeenUseCase.Result.StaleVersion) again).current()).isEqualTo(resource);
        assertThat(repository.findById(stored.id())).contains(resource);
    }

    @Test
    void aCallerThatSendsNoPreconditionAppliesUnconditionallyAndCanRepeat() {
        var stored = repository.save(new Note(
                1L,
                "sample",
                true,
                1L));
        var command = new MarkSeenCommand(
                stored.id());

        // `null` is not a version, and not a wrong one: it is the absence of
        // a precondition, which this transition was asked to allow.
        assertThat(useCase.execute(stored.id(), command, null))
                .isInstanceOf(MarkSeenUseCase.Result.Applied.class);

        // Again. A guarded call would be stale by now -- the row moved -- so
        // this is the assertion that fails if the guard stops being optional.
        assertThat(useCase.execute(stored.id(), command, null))
                .isInstanceOf(MarkSeenUseCase.Result.Applied.class);
    }
}
