package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryContactRepository;
import com.example.intercom.domain.Contact;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class CreateContactUseCaseTest {

    private final InMemoryContactRepository repository = new InMemoryContactRepository();
    private final CreateContactUseCase useCase = new DefaultCreateContactUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        CreateContactCommand command = new CreateContactCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                Optional.empty());

        Contact created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.email()).isEqualTo(command.email());
        assertThat(created.displayName()).isEqualTo(command.displayName());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
