package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryConversationRepository;
import com.example.intercom.domain.Conversation;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class OpenConversationUseCaseTest {

    private final InMemoryConversationRepository repository = new InMemoryConversationRepository();
    private final OpenConversationUseCase useCase = new DefaultOpenConversationUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        OpenConversationCommand command = new OpenConversationCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"));

        Conversation created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.contactId()).isEqualTo(command.contactId());
        assertThat(created.inboxId()).isEqualTo(command.inboxId());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
