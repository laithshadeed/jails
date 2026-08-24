package com.example.paymentsgateway.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import com.example.paymentsgateway.service.CapturePaymentCommand;
import com.example.paymentsgateway.service.CapturePaymentUseCase;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcCapturePaymentTransitionIT {

    @Autowired private PaymentRepository repository;
    @Autowired private CapturePaymentUseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {
        repository.save(new Payment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0],
                PaymentStatus.values()[0],
                1L,
                Optional.empty(),
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z")));
        var command = new CapturePaymentCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                PaymentStatus.values()[0],
                1L);

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf(CapturePaymentUseCase.StaleVersionException.class);
        assertThat(repository.findById(String.valueOf(command.id())))
                .get().extracting(Payment::version)
                .isEqualTo(updated.version());
    }

    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {
        var stored = new Payment(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0],
                PaymentStatus.values()[0],
                1L,
                Optional.empty(),
                Optional.empty(),
                Instant.parse("2024-01-01T00:00:00Z"),
                Instant.parse("2024-01-01T00:00:00Z"));
        repository.save(stored);
        var wrongScope = new CapturePaymentCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000002"),
                PaymentStatus.values()[0],
                1L);

        assertThatThrownBy(() -> useCase.execute(wrongScope))
                .isInstanceOf(CapturePaymentUseCase.NotFoundException.class);
        assertThat(repository.findById(String.valueOf(stored.id()))).contains(stored);
    }

}
