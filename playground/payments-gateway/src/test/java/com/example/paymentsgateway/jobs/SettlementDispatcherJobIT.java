package com.example.paymentsgateway.jobs;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.paymentsgateway.TestcontainersConfig;
import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.domain.PaymentMethod;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class SettlementDispatcherJobIT {

    @Autowired private SettlementDispatcherQueue queue;
    @Autowired private SettlementDispatcherWorker worker;
    @Autowired private JdbcSettlementDispatcherStore store;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;
    @Autowired private PaymentRepository results;

    @Test
    void committedWorkRunsAndRepeatingTheSameIdIsIdempotent() {
        var work = new SettlementDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0]);

        queue.enqueue(work);
        queue.enqueue(work);
        worker.runOnce();

        assertThat(results.findById(String.valueOf(work.id()))).isPresent();
        assertThat(queue.status(work.id())).get()
                .extracting(SettlementDispatcherQueue.Status::state)
                .isEqualTo(SettlementDispatcherQueue.State.SUCCEEDED);
    }

    @Test
    void anExpiredLeaseIsReclaimedAndBoundedFailureBecomesVisible() {
        var work = new SettlementDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0]);
        queue.enqueue(work);

        assertThat(store.claim()).isPresent();
        db.sql("update settlement_dispatcher_jobs set lease_until = now() - interval '1 second' where id = :id")
                .param("id", work.id())
                .update();
        var reclaimed = store.claim().orElseThrow();
        store.fail(work.id(), new IllegalStateException("test failure"));

        assertThat(reclaimed.attempt()).isEqualTo(2);
        assertThat(queue.status(work.id())).get()
                .satisfies(status -> {
                    assertThat(status.state()).isEqualTo(SettlementDispatcherQueue.State.FAILED);
                    assertThat(status.lastError()).contains("test failure");
                });
    }

    @Test
    void reusingAnIdWithDifferentPayloadIsAConflict() {
        var original = new SettlementDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0]);
        var conflicting = new SettlementDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000002"),
                "sample",
                1L,
                "sample",
                PaymentMethod.values()[0]);

        queue.enqueue(original);

        assertThatThrownBy(() -> queue.enqueue(conflicting))
                .isInstanceOf(SettlementDispatcherQueue.IdempotencyConflictException.class);
    }

}
