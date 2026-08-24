package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryWorkspaceRepository;
import com.example.intercom.domain.Workspace;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class CreateWorkspaceUseCaseTest {

    private final InMemoryWorkspaceRepository repository = new InMemoryWorkspaceRepository();
    private final CreateWorkspaceUseCase useCase = new DefaultCreateWorkspaceUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        CreateWorkspaceCommand command = new CreateWorkspaceCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample");

        Workspace created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.name()).isEqualTo(command.name());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
