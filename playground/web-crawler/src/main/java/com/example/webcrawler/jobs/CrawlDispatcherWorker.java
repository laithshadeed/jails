package com.example.webcrawler.jobs;

import com.example.webcrawler.app.CrawlRunRepository;
import com.example.webcrawler.service.QueueCrawlCommand;
import com.example.webcrawler.service.QueueCrawlUseCase;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** At-least-once worker; an expired lease is reclaimed after process death. */
@Component
public final class CrawlDispatcherWorker {

    private static final Logger log = LoggerFactory.getLogger(CrawlDispatcherWorker.class);
    private final JdbcCrawlDispatcherStore store;
    private final QueueCrawlUseCase useCase;
    private final CrawlRunRepository results;

    public CrawlDispatcherWorker(JdbcCrawlDispatcherStore store, QueueCrawlUseCase useCase,
                       CrawlRunRepository results) {
        this.store = store;
        this.useCase = useCase;
        this.results = results;
    }

    @Scheduled(
            fixedDelayString = "${jobs.crawl-dispatcher.delay:PT1S}",
            initialDelayString = "${jobs.crawl-dispatcher.initial-delay:PT1S}")
    public void run() {
        try {
            runOnce();
        } catch (RuntimeException infrastructureFailure) {
            log.error("CrawlDispatcher could not claim durable work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() {
        store.claim().ifPresent(this::execute);
    }

    private void execute(JdbcCrawlDispatcherStore.Claimed claimed) {
        var work = claimed.work();
        try {
            // A process can die after the use-case transaction commits and
            // before this queue row is acknowledged. The stable shared id is
            // the recovery proof: do not repeat an already-visible effect.
            if (results.findById(String.valueOf(work.id())).isEmpty()) {
                useCase.execute(new QueueCrawlCommand(
                    work.id(),
                    work.seedUrl()));
            }
            store.succeed(work.id());
        } catch (RuntimeException failure) {
            store.fail(work.id(), failure);
            log.warn("CrawlDispatcher attempt {} failed", claimed.attempt(), failure);
        }
    }
}
