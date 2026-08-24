package com.example.paymentsgateway.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.adapters.InMemoryPaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class AuthorisePaymentUseCaseTest {

    private final InMemoryPaymentRepository repository = new InMemoryPaymentRepository();
    private final AuthorisePaymentUseCase useCase = new DefaultAuthorisePaymentUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        AuthorisePaymentCommand command = new AuthorisePaymentCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0]);

        Payment created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.merchantId()).isEqualTo(command.merchantId());
        assertThat(created.idempotencyKey()).isEqualTo(command.idempotencyKey());
        assertThat(created.amountMinor()).isEqualTo(command.amountMinor());
        assertThat(created.currency()).isEqualTo(command.currency());
        assertThat(created.method()).isEqualTo(command.method());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
