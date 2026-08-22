package com.example.demo.jobs;

import com.example.demo.app.ItemRepository;
import com.example.demo.service.AddItemCommand;
import com.example.demo.service.AddItemUseCase;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** At-least-once worker; an expired lease is reclaimed after process death. */
@Component
public final class ItemDispatcherWorker {

    private static final Logger log = LoggerFactory.getLogger(ItemDispatcherWorker.class);
    private final JdbcItemDispatcherStore store;
    private final AddItemUseCase useCase;
    private final ItemRepository results;

    public ItemDispatcherWorker(JdbcItemDispatcherStore store, AddItemUseCase useCase,
                       ItemRepository results) {
        this.store = store;
        this.useCase = useCase;
        this.results = results;
    }

    @Scheduled(
            fixedDelayString = "${jobs.item-dispatcher.delay:PT1S}",
            initialDelayString = "${jobs.item-dispatcher.initial-delay:PT1S}")
    public void run() {
        try {
            runOnce();
        } catch (RuntimeException infrastructureFailure) {
            log.error("ItemDispatcher could not claim durable work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() {
        store.claim().ifPresent(this::execute);
    }

    private void execute(JdbcItemDispatcherStore.Claimed claimed) {
        var work = claimed.work();
        try {
            // A process can die after the use-case transaction commits and
            // before this queue row is acknowledged. The stable shared id is
            // the recovery proof: do not repeat an already-visible effect.
            if (results.findById(String.valueOf(work.id())).isEmpty()) {
                useCase.execute(new AddItemCommand(
                    work.id(),
                    work.ownerId(),
                    work.name()));
            }
            store.succeed(work.id());
        } catch (RuntimeException failure) {
            store.fail(work.id(), failure);
            log.warn("ItemDispatcher attempt {} failed", claimed.attempt(), failure);
        }
    }
}
