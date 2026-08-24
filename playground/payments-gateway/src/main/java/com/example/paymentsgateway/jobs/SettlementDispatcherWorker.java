package com.example.paymentsgateway.jobs;

import com.example.paymentsgateway.app.PaymentRepository;
import com.example.paymentsgateway.service.AuthorisePaymentCommand;
import com.example.paymentsgateway.service.AuthorisePaymentUseCase;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** At-least-once worker; an expired lease is reclaimed after process death. */
@Component
public final class SettlementDispatcherWorker {

    private static final Logger log = LoggerFactory.getLogger(SettlementDispatcherWorker.class);
    private final JdbcSettlementDispatcherStore store;
    private final AuthorisePaymentUseCase useCase;
    private final PaymentRepository results;

    public SettlementDispatcherWorker(JdbcSettlementDispatcherStore store, AuthorisePaymentUseCase useCase,
                       PaymentRepository results) {
        this.store = store;
        this.useCase = useCase;
        this.results = results;
    }

    @Scheduled(
            fixedDelayString = "${jobs.settlement-dispatcher.delay:PT1S}",
            initialDelayString = "${jobs.settlement-dispatcher.initial-delay:PT1S}")
    public void run() {
        try {
            runOnce();
        } catch (RuntimeException infrastructureFailure) {
            log.error("SettlementDispatcher could not claim durable work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() {
        store.claim().ifPresent(this::execute);
    }

    private void execute(JdbcSettlementDispatcherStore.Claimed claimed) {
        var work = claimed.work();
        try {
            // A process can die after the use-case transaction commits and
            // before this queue row is acknowledged. The stable shared id is
            // the recovery proof: do not repeat an already-visible effect.
            if (results.findById(String.valueOf(work.id())).isEmpty()) {
                useCase.execute(new AuthorisePaymentCommand(
                    work.id(),
                    work.merchantId(),
                    work.idempotencyKey(),
                    work.amountMinor(),
                    work.currency(),
                    work.method()));
            }
            store.succeed(work.id());
        } catch (RuntimeException failure) {
            store.fail(work.id(), failure);
            log.warn("SettlementDispatcher attempt {} failed", claimed.attempt(), failure);
        }
    }
}
