package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.adapters.InMemoryItemRepository;
import com.example.demo.domain.Item;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class AddItemUseCaseTest {

    private final InMemoryItemRepository repository = new InMemoryItemRepository();
    private final AddItemUseCase useCase = new StoringAddItemUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        AddItemCommand command = new AddItemCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample");

        Item created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.ownerId()).isEqualTo(command.ownerId());
        assertThat(created.name()).isEqualTo(command.name());
        assertThat(repository.findById(created.id())).contains(created);
    }
}
