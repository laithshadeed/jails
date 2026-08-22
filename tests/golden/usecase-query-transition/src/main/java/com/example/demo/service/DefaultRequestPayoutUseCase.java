package com.example.demo.service;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultRequestPayoutUseCase implements RequestPayoutUseCase {

    private final PayoutRepository repository;

    public DefaultRequestPayoutUseCase(PayoutRepository repository) {
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
