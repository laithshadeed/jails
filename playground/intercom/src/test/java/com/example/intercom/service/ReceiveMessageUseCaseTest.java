package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryMessageRepository;
import com.example.intercom.domain.Message;
import com.example.intercom.domain.MessageDirection;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class ReceiveMessageUseCaseTest {

    private final InMemoryMessageRepository repository = new InMemoryMessageRepository();
    private final ReceiveMessageUseCase useCase = new DefaultReceiveMessageUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        ReceiveMessageCommand command = new ReceiveMessageCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                MessageDirection.values()[0],
                "sample");

        Message created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.conversationId()).isEqualTo(command.conversationId());
        assertThat(created.direction()).isEqualTo(command.direction());
        assertThat(created.body()).isEqualTo(command.body());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
