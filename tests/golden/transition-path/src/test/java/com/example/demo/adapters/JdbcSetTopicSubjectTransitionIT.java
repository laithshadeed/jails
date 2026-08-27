package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.app.TopicRepository;
import com.example.demo.domain.Topic;
import com.example.demo.service.SetTopicSubjectCommand;
import com.example.demo.service.SetTopicSubjectUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcSetTopicSubjectTransitionIT {

    @Autowired private TopicRepository repository;
    @Autowired private SetTopicSubjectUseCase useCase;

    @Test
    void appliesOnceAndReportsTheStaleVersionWithoutAnotherMutation() {
        var stored = repository.save(new Topic(
                1L,
                1L,
                "sample",
                1L));
        var command = new SetTopicSubjectCommand(
                "sample");

        var applied = useCase.execute(stored.userId(), command, 1L);

        assertThat(applied).isInstanceOf(SetTopicSubjectUseCase.Result.Applied.class);
        var resource = ((SetTopicSubjectUseCase.Result.Applied) applied).resource();
        assertThat(resource.version()).isEqualTo(1L + 1);

        // The same expectation a second time is stale, and the outcome
        // carries the row as it now stands rather than a message about it.
        var again = useCase.execute(stored.userId(), command, 1L);
        assertThat(again).isInstanceOf(SetTopicSubjectUseCase.Result.StaleVersion.class);
        assertThat(((SetTopicSubjectUseCase.Result.StaleVersion) again).current()).isEqualTo(resource);
        assertThat(repository.findById(stored.id())).contains(resource);
    }
}
