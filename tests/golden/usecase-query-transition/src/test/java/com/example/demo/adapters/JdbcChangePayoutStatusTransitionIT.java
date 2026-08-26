package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

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
    void appliesOnceAndReportsTheStaleVersionWithoutAnotherMutation() {
        var stored = repository.save(new Payout(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L,
                PayoutStatus.values()[0],
                1L,
                Instant.parse("2024-01-01T00:00:00Z")));
        var command = new ChangePayoutStatusCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                PayoutStatus.values()[0]);

        var applied = useCase.execute(command, 1L);

        assertThat(applied).isInstanceOf(ChangePayoutStatusUseCase.Result.Applied.class);
        var resource = ((ChangePayoutStatusUseCase.Result.Applied) applied).resource();
        assertThat(resource.version()).isEqualTo(1L + 1);

        // The same expectation a second time is stale, and the outcome
        // carries the row as it now stands rather than a message about it.
        var again = useCase.execute(command, 1L);
        assertThat(again).isInstanceOf(ChangePayoutStatusUseCase.Result.StaleVersion.class);
        assertThat(((ChangePayoutStatusUseCase.Result.StaleVersion) again).current()).isEqualTo(resource);
        assertThat(repository.findById(command.id())).contains(resource);
    }
}
