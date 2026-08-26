package com.example.demo.service;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;

/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code OutboxRequestPayoutUseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class StoringRequestPayoutUseCase implements RequestPayoutUseCase {

    private final PayoutRepository repository;

    public StoringRequestPayoutUseCase(PayoutRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Override
    public Payout execute(RequestPayoutCommand command) {
        Objects.requireNonNull(command, "command is required");
        Payout payout = new Payout(
                command.id(),
                command.amount(),
                PayoutStatus.values()[0],
                0L,
                Instant.now());
        repository.save(payout);
        return payout;
    }
}
