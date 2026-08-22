package com.example.demo.jobs;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.demo.app.ItemRepository;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest(properties = {
        "jobs.item-dispatcher.initial-delay=PT1H",
        "jobs.item-dispatcher.max-attempts=2"
})
@org.springframework.transaction.annotation.Transactional
class ItemDispatcherJobIT {

    @Autowired private ItemDispatcherQueue queue;
    @Autowired private ItemDispatcherWorker worker;
    @Autowired private JdbcItemDispatcherStore store;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;
    @Autowired private ItemRepository results;

    @Test
    void committedWorkRunsAndRepeatingTheSameIdIsIdempotent() {
        var work = new ItemDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample");

        queue.enqueue(work);
        queue.enqueue(work);
        worker.runOnce();

        assertThat(results.findById(String.valueOf(work.id()))).isPresent();
        assertThat(queue.status(work.id())).get()
                .extracting(ItemDispatcherQueue.Status::state)
                .isEqualTo(ItemDispatcherQueue.State.SUCCEEDED);
    }

    @Test
    void anExpiredLeaseIsReclaimedAndBoundedFailureBecomesVisible() {
        var work = new ItemDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample");
        queue.enqueue(work);

        assertThat(store.claim()).isPresent();
        db.sql("update item_dispatcher_jobs set lease_until = now() - interval '1 second' where id = :id")
                .param("id", work.id())
                .update();
        var reclaimed = store.claim().orElseThrow();
        store.fail(work.id(), new IllegalStateException("test failure"));

        assertThat(reclaimed.attempt()).isEqualTo(2);
        assertThat(queue.status(work.id())).get()
                .satisfies(status -> {
                    assertThat(status.state()).isEqualTo(ItemDispatcherQueue.State.FAILED);
                    assertThat(status.lastError()).contains("test failure");
                });
    }

    @Test
    void reusingAnIdWithDifferentPayloadIsAConflict() {
        var original = new ItemDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample");
        var conflicting = new ItemDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000002"),
                "sample");

        queue.enqueue(original);

        assertThatThrownBy(() -> queue.enqueue(conflicting))
                .isInstanceOf(ItemDispatcherQueue.IdempotencyConflictException.class);
    }

}
