package com.example.webcrawler.jobs;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.webcrawler.TestcontainersConfig;
import com.example.webcrawler.app.CrawlRunRepository;
import java.net.URI;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class CrawlDispatcherJobIT {

    @Autowired private CrawlDispatcherQueue queue;
    @Autowired private CrawlDispatcherWorker worker;
    @Autowired private JdbcCrawlDispatcherStore store;
    @Autowired private org.springframework.jdbc.core.simple.JdbcClient db;
    @Autowired private CrawlRunRepository results;

    @Test
    void committedWorkRunsAndRepeatingTheSameIdIsIdempotent() {
        var work = new CrawlDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"));

        queue.enqueue(work);
        queue.enqueue(work);
        worker.runOnce();

        assertThat(results.findById(String.valueOf(work.id()))).isPresent();
        assertThat(queue.status(work.id())).get()
                .extracting(CrawlDispatcherQueue.Status::state)
                .isEqualTo(CrawlDispatcherQueue.State.SUCCEEDED);
    }

    @Test
    void anExpiredLeaseIsReclaimedAndBoundedFailureBecomesVisible() {
        var work = new CrawlDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"));
        queue.enqueue(work);

        assertThat(store.claim()).isPresent();
        db.sql("update crawl_dispatcher_jobs set lease_until = now() - interval '1 second' where id = :id")
                .param("id", work.id())
                .update();
        var reclaimed = store.claim().orElseThrow();
        store.fail(work.id(), new IllegalStateException("test failure"));

        assertThat(reclaimed.attempt()).isEqualTo(2);
        assertThat(queue.status(work.id())).get()
                .satisfies(status -> {
                    assertThat(status.state()).isEqualTo(CrawlDispatcherQueue.State.FAILED);
                    assertThat(status.lastError()).contains("test failure");
                });
    }

    @Test
    void reusingAnIdWithDifferentPayloadIsAConflict() {
        var original = new CrawlDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"));
        var conflicting = new CrawlDispatcherWork(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://different.example.test/"));

        queue.enqueue(original);

        assertThatThrownBy(() -> queue.enqueue(conflicting))
                .isInstanceOf(CrawlDispatcherQueue.IdempotencyConflictException.class);
    }

}
