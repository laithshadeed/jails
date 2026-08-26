package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.demo.app.PayoutRepository;
import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.ChangePayoutStatusCommand;
import com.example.demo.service.ChangePayoutStatusUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class JdbcChangePayoutStatusTransitionIT {

    @Autowired private PayoutRepository repository;
    @Autowired private ChangePayoutStatusUseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {
        repository.save(new Payout(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L,
                PayoutStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z")));
        var command = new ChangePayoutStatusCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                PayoutStatus.values()[0],
                1L);

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf(ChangePayoutStatusUseCase.StaleVersionException.class);
        assertThat(repository.findById(command.id()))
                .get().extracting(Payout::version)
                .isEqualTo(updated.version());
    }

}
