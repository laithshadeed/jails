package com.example.demo.service;

import com.example.demo.app.TicketRepository;
import com.example.demo.domain.Ticket;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code OutboxOpenTicketUseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class StoringOpenTicketUseCase implements OpenTicketUseCase {

    private final TicketRepository repository;

    public StoringOpenTicketUseCase(TicketRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Ticket execute(OpenTicketCommand command) {
        Objects.requireNonNull(command, "command is required");
        Ticket ticket = new Ticket(
                0L,
                command.subject());
        return repository.save(ticket);
    }
}
