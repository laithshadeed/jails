package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.ConversationAssignmentRepository;
import com.example.intercom.domain.AssignmentStatus;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.ReassignConversationCommand;
import com.example.intercom.service.ReassignConversationUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcReassignConversationTransitionIT {

    @Autowired private ConversationAssignmentRepository repository;
    @Autowired private ReassignConversationUseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {
        repository.save(new ConversationAssignment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                AssignmentStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z")));
        var command = new ReassignConversationCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                AssignmentStatus.values()[0],
                1L);

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf(ReassignConversationUseCase.StaleVersionException.class);
        assertThat(repository.findById(String.valueOf(command.id())))
                .get().extracting(ConversationAssignment::version)
                .isEqualTo(updated.version());
    }

    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {
        var stored = new ConversationAssignment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                AssignmentStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);
        var wrongScope = new ReassignConversationCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000002"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                AssignmentStatus.values()[0],
                1L);

        assertThatThrownBy(() -> useCase.execute(wrongScope))
                .isInstanceOf(ReassignConversationUseCase.NotFoundException.class);
        assertThat(repository.findById(String.valueOf(stored.id()))).contains(stored);
    }

}
