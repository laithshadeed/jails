package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryInboxRepository;
import com.example.intercom.domain.Inbox;
import com.example.intercom.domain.InboxChannel;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class CreateInboxUseCaseTest {

    private final InMemoryInboxRepository repository = new InMemoryInboxRepository();
    private final CreateInboxUseCase useCase = new DefaultCreateInboxUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        CreateInboxCommand command = new CreateInboxCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                InboxChannel.values()[0]);

        Inbox created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.name()).isEqualTo(command.name());
        assertThat(created.channel()).isEqualTo(command.channel());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
