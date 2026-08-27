package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.adapters.InMemoryTicketRepository;
import com.example.demo.domain.Ticket;
import org.junit.jupiter.api.Test;

class OpenTicketUseCaseTest {

    private final InMemoryTicketRepository repository = new InMemoryTicketRepository();
    private final OpenTicketUseCase useCase = new StoringOpenTicketUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        OpenTicketCommand command = new OpenTicketCommand(
                "sample");

        Ticket created = useCase.execute(command);

        assertThat(created.id()).isPositive();
        assertThat(created.subject()).isEqualTo(command.subject());
        assertThat(repository.findById(created.id())).contains(created);
    }

    /**
     * missing.md M3: two creates are two rows. When the key was
     * constructed rather than assigned, this was two creates and
     * *one* row, with no exception and no log line.
     */
    @Test
    void twoCreatesAreTwoRows() {
        OpenTicketCommand command = new OpenTicketCommand(
                "sample");

        Ticket first = useCase.execute(command);
        Ticket second = useCase.execute(command);

        assertThat(second.id()).isNotEqualTo(first.id());
        assertThat(repository.findAll()).hasSize(2);
    }
}
