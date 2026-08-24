package com.example.paymentsgateway.service;

import com.example.paymentsgateway.app.RefundRepository;
import com.example.paymentsgateway.domain.Refund;
import java.time.Instant;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultRefundPaymentRequestUseCase implements RefundPaymentRequestUseCase {

    private final RefundRepository repository;

    public DefaultRefundPaymentRequestUseCase(RefundRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Refund execute(RefundPaymentRequestCommand command) {
        Objects.requireNonNull(command, "command is required");
        Refund refund = new Refund(
                command.id(),
                command.merchantId(),
                command.paymentId(),
                command.amountMinor(),
                Optional.empty(),
                Instant.now(),
                Instant.now());
        repository.save(refund);
        return refund;
    }
}
