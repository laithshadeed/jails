package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.adapters.InMemoryPayoutRepository;
import com.example.demo.domain.Payout;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class RequestPayoutUseCaseTest {

    private final InMemoryPayoutRepository repository = new InMemoryPayoutRepository();
    private final RequestPayoutUseCase useCase = new StoringRequestPayoutUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        RequestPayoutCommand command = new RequestPayoutCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L);

        Payout created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.amount()).isEqualTo(command.amount());
        assertThat(repository.findById(created.id())).contains(created);
    }
}
