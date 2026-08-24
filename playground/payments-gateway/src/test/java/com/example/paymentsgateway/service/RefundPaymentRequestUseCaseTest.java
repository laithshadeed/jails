package com.example.paymentsgateway.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.adapters.InMemoryRefundRepository;
import com.example.paymentsgateway.domain.Refund;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class RefundPaymentRequestUseCaseTest {

    private final InMemoryRefundRepository repository = new InMemoryRefundRepository();
    private final RefundPaymentRequestUseCase useCase = new DefaultRefundPaymentRequestUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        RefundPaymentRequestCommand command = new RefundPaymentRequestCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L);

        Refund created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.merchantId()).isEqualTo(command.merchantId());
        assertThat(created.paymentId()).isEqualTo(command.paymentId());
        assertThat(created.amountMinor()).isEqualTo(command.amountMinor());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
