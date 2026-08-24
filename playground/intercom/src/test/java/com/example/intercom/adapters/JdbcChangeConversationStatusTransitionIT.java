package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.intercom.TestcontainersConfig;
import com.example.intercom.app.ConversationRepository;
import com.example.intercom.domain.Conversation;
import com.example.intercom.domain.ConversationStatus;
import com.example.intercom.service.ChangeConversationStatusCommand;
import com.example.intercom.service.ChangeConversationStatusUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcChangeConversationStatusTransitionIT {

    @Autowired private ConversationRepository repository;
    @Autowired private ChangeConversationStatusUseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {
        repository.save(new Conversation(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                ConversationStatus.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                1L,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z")));
        var command = new ChangeConversationStatusCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                ConversationStatus.values()[0],
                1L);

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf(ChangeConversationStatusUseCase.StaleVersionException.class);
        assertThat(repository.findById(String.valueOf(command.id())))
                .get().extracting(Conversation::version)
                .isEqualTo(updated.version());
    }

    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {
        var stored = new Conversation(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                ConversationStatus.values()[0],
                Instant.parse("2024-01-01T00:00:00Z"),
                1L,
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);
        var wrongScope = new ChangeConversationStatusCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000002"),
                ConversationStatus.values()[0],
                1L);

        assertThatThrownBy(() -> useCase.execute(wrongScope))
                .isInstanceOf(ChangeConversationStatusUseCase.NotFoundException.class);
        assertThat(repository.findById(String.valueOf(stored.id()))).contains(stored);
    }

}
