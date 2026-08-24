package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.jobs.JdbcAuthorisePaymentOutbox;
import com.example.paymentsgateway.messaging.PaymentAuthorisedEvent;
import java.time.Instant;
import java.util.Objects;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** Creates the resource and stages its event in the same database transaction. */
@Primary
@Component
public class OutboxAuthorisePaymentUseCase implements AuthorisePaymentUseCase {

    private final DefaultAuthorisePaymentUseCase delegate;
    private final JdbcAuthorisePaymentOutbox outbox;

    public OutboxAuthorisePaymentUseCase(DefaultAuthorisePaymentUseCase delegate, JdbcAuthorisePaymentOutbox outbox) {
        this.delegate = Objects.requireNonNull(delegate, "delegate is required");
        this.outbox = Objects.requireNonNull(outbox, "outbox is required");
    }

    @Override
    @Transactional
    public Payment execute(AuthorisePaymentCommand command) {
        var result = delegate.execute(command);
        outbox.stage(new PaymentAuthorisedEvent(
                result.id(),
                result.merchantId(),
                result.id(),
                Instant.now()));
        return result;
    }
}
