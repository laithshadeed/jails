package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.AuthorRepository;
import com.example.demo.domain.Author;
import com.example.demo.service.PostNoteCommand;
import com.example.demo.service.PostNoteUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class ResolvingPostNoteUseCaseIT {

    @Autowired private AuthorRepository parents;
    @Autowired private PostNoteUseCase useCase;

    @Test
    void resolvesTheKeyFromTheParentAndReportsWhenThereIsNoParent() {
        var command = new PostNoteCommand(
                "sample",
                "sample");

        // Nothing to resolve against yet: the empty result is the answer.
        assertThat(useCase.execute(command)).isEmpty();

        var parent = parents.save(new Author(
                1L,
                "sample"));

        var created = useCase.execute(command);
        assertThat(created).isPresent();
        assertThat(created.orElseThrow().authorId()).isEqualTo(parent.id());
    }
}
