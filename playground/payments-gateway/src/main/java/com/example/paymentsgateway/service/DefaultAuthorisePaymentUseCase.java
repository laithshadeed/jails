package com.example.paymentsgateway.service;

import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentStatus;
import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultAuthorisePaymentUseCase implements AuthorisePaymentUseCase {

    private final PaymentRepository repository;

    public DefaultAuthorisePaymentUseCase(PaymentRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Payment execute(AuthorisePaymentCommand command) {
        Objects.requireNonNull(command, "command is required");
        Payment payment = new Payment(
                command.id(),
                command.merchantId(),
                command.idempotencyKey(),
                command.amountMinor(),
                command.currency(),
                command.method(),
                PaymentStatus.values()[0],
                0L,
                Optional.empty(),
                Optional.empty(),
                Instant.now(),
                Instant.now());
        repository.save(payment);
        return payment;
    }
}
