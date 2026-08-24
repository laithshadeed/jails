package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryConversationAssignmentRepository;
import com.example.intercom.domain.ConversationAssignment;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class AssignConversationUseCaseTest {

    private final InMemoryConversationAssignmentRepository repository = new InMemoryConversationAssignmentRepository();
    private final AssignConversationUseCase useCase = new DefaultAssignConversationUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        AssignConversationCommand command = new AssignConversationCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"));

        ConversationAssignment created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.conversationId()).isEqualTo(command.conversationId());
        assertThat(created.memberId()).isEqualTo(command.memberId());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
